//! Schema types, extraction, and tracking for roam wire protocol.
//!
//! This module contains:
//! - Schema data types (TypeSchemaId, Schema, SchemaKind, etc.)
//! - CBOR serialization for schema messages
//! - Schema extraction from facet Shape graphs
//! - SchemaTracker for per-connection sent/received tracking

use facet::Facet;
use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::{MethodId, is_rx, is_tx};

// ============================================================================
// Schema data types
// ============================================================================

/// An opaque type identifier, unique within a connection half.
///
/// Type IDs are assigned by the sender (typically as incrementing integers)
/// and are used so schemas can reference each other and to track which types
/// have already been sent. They are not stable across connections.
// r[impl schema.type-id]
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeSchemaId(pub u32);

/// The root schema type describing a single type.
#[derive(Facet, Clone, Debug)]
pub struct Schema {
    pub type_id: TypeSchemaId,
    /// The Rust type name (e.g. "Point", "Vec<String>"). For diagnostics.
    #[facet(default)]
    pub name: String,
    pub kind: SchemaKind,
}

/// The structural kind of a type.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SchemaKind {
    Struct {
        fields: Vec<FieldSchema>,
    },
    Enum {
        variants: Vec<VariantSchema>,
    },
    Tuple {
        elements: Vec<TypeSchemaId>,
    },
    List {
        element: TypeSchemaId,
    },
    Map {
        key: TypeSchemaId,
        value: TypeSchemaId,
    },
    Set {
        element: TypeSchemaId,
    },
    Array {
        element: TypeSchemaId,
        length: u64,
    },
    Option {
        element: TypeSchemaId,
    },
    Primitive {
        primitive_type: PrimitiveType,
    },
}

/// Describes a single field in a struct or struct variant.
#[derive(Facet, Clone, Debug)]
pub struct FieldSchema {
    pub name: String,
    pub type_id: TypeSchemaId,
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
    Newtype { type_id: TypeSchemaId },
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

/// Lookup table mapping TypeSchemaId → Schema, used for resolving type
/// references during deserialization with translation plans.
pub type SchemaRegistry = HashMap<TypeSchemaId, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.type_id, s.clone())).collect()
}

/// Binds a method (by its MethodId as u64) to the root TypeSchemaId of the
/// type being sent for that method. Sent once per method per direction.
#[derive(Facet, Clone, Debug)]
pub struct MethodSchemaBinding {
    pub method_id: u64,
    pub root_type_schema_id: TypeSchemaId,
    /// Whether this binding is for args (caller → callee) or response (callee → caller).
    pub direction: BindingDirection,
}

/// Whether a method schema binding describes args or the response type.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BindingDirection {
    /// The sender will send data of this type as method arguments.
    Args,
    /// The sender will send data of this type as the method response.
    Response,
}

/// CBOR-encoded payload inside a schema wire message.
/// A struct so new fields can be added without breaking the wire format.
#[derive(Facet, Clone, Debug)]
pub struct SchemaMessagePayload {
    pub schemas: Vec<Schema>,
    #[facet(default)]
    pub method_bindings: Vec<MethodSchemaBinding>,
}

/// Build a CBOR-encoded schema message.
// r[impl schema.format.self-contained]
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn build_schema_message(
    schemas: &[Schema],
    method_bindings: &[MethodSchemaBinding],
) -> Vec<u8> {
    let payload = SchemaMessagePayload {
        schemas: schemas.to_vec(),
        method_bindings: method_bindings.to_vec(),
    };
    facet_cbor::to_vec(&payload).expect("schema CBOR serialization should not fail")
}

/// Parse a CBOR-encoded schema message.
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn parse_schema_message(bytes: &[u8]) -> Result<SchemaMessagePayload, facet_cbor::CborError> {
    facet_cbor::from_slice(bytes)
}

// ============================================================================
// Schema extraction
// ============================================================================

/// What `prepare_send_for_method` returns when there's something to send.
pub struct PreparedSchemaMessage {
    pub schemas: Vec<Schema>,
    pub method_bindings: Vec<MethodSchemaBinding>,
}

/// Tracks schema exchange state for one connection.
///
/// Handles both outbound dedup (what we've sent) and inbound storage
/// (schemas received from the remote peer, used for building translation plans).
// r[impl schema.tracking.sent]
// r[impl schema.tracking.received]
// r[impl schema.type-id.per-connection]
pub struct SchemaTracker {
    /// Type schema IDs we've already sent.
    sent: Mutex<HashSet<TypeSchemaId>>,
    /// Method args bindings we've already sent (by method_id).
    args_bindings_sent: Mutex<HashSet<u64>>,
    /// Method response bindings we've already sent (by method_id).
    response_bindings_sent: Mutex<HashSet<u64>>,
    /// Assigns incrementing type IDs to shapes we extract.
    shape_to_id: Mutex<HashMap<&'static Shape, TypeSchemaId>>,
    /// Next ID to assign.
    next_id: Mutex<u32>,
    /// Type schemas received from the remote peer.
    received: Mutex<HashMap<TypeSchemaId, Schema>>,
    /// Args bindings received: method_id → root TypeSchemaId for args.
    received_args_bindings: Mutex<HashMap<u64, TypeSchemaId>>,
    /// Response bindings received: method_id → root TypeSchemaId for response.
    received_response_bindings: Mutex<HashMap<u64, TypeSchemaId>>,
}

impl SchemaTracker {
    pub fn new() -> Self {
        SchemaTracker {
            sent: Mutex::new(HashSet::new()),
            args_bindings_sent: Mutex::new(HashSet::new()),
            response_bindings_sent: Mutex::new(HashSet::new()),
            shape_to_id: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
            received: Mutex::new(HashMap::new()),
            received_args_bindings: Mutex::new(HashMap::new()),
            received_response_bindings: Mutex::new(HashMap::new()),
        }
    }

    /// Allocate or look up a TypeSchemaId for a Shape.
    fn id_for_shape(&self, shape: &'static Shape) -> TypeSchemaId {
        let mut map = self.shape_to_id.lock().unwrap();
        if let Some(&id) = map.get(shape) {
            return id;
        }
        let mut next = self.next_id.lock().unwrap();
        let id = TypeSchemaId(*next);
        *next += 1;
        map.insert(shape, id);
        id
    }

    /// Prepare type schemas and method binding for a method call/response.
    ///
    /// Returns `Some(...)` if there's anything to send (unsent type schemas
    /// or a first-time method binding). Returns `None` if everything was
    /// already sent.
    // r[impl schema.tracking.transitive]
    // r[impl schema.exchange.idempotent]
    // r[impl schema.principles.once-per-type]
    // r[impl schema.principles.sender-driven]
    // r[impl schema.principles.no-roundtrips]
    pub fn prepare_send_for_method(
        &self,
        method_id: MethodId,
        shape: &'static Shape,
        direction: BindingDirection,
    ) -> Option<PreparedSchemaMessage> {
        let all_schemas = self.extract_schemas(shape);
        let root_type_schema_id = all_schemas.last().map(|s| s.type_id)?;

        let mut sent = self.sent.lock().unwrap();
        let unsent: Vec<Schema> = all_schemas
            .into_iter()
            .filter(|s| !sent.contains(&s.type_id))
            .collect();

        let mut bindings_sent = match direction {
            BindingDirection::Args => self.args_bindings_sent.lock().unwrap(),
            BindingDirection::Response => self.response_bindings_sent.lock().unwrap(),
        };
        let need_method_binding = bindings_sent.insert(method_id.0);

        if unsent.is_empty() && !need_method_binding {
            return None;
        }

        for s in &unsent {
            sent.insert(s.type_id);
        }

        let method_bindings = if need_method_binding {
            vec![MethodSchemaBinding {
                method_id: method_id.0,
                root_type_schema_id,
                direction,
            }]
        } else {
            vec![]
        };

        Some(PreparedSchemaMessage {
            schemas: unsent,
            method_bindings,
        })
    }

    /// Record a parsed schema message from the remote peer.
    pub fn record_received(&self, payload: SchemaMessagePayload) {
        {
            let mut received = self.received.lock().unwrap();
            for schema in payload.schemas {
                received.insert(schema.type_id, schema);
            }
        }
        for binding in payload.method_bindings {
            let map = match binding.direction {
                BindingDirection::Args => &self.received_args_bindings,
                BindingDirection::Response => &self.received_response_bindings,
            };
            map.lock()
                .unwrap()
                .insert(binding.method_id, binding.root_type_schema_id);
        }
    }

    /// Look up the remote's root TypeSchemaId for a method's args.
    pub fn get_remote_args_root(&self, method_id: MethodId) -> Option<TypeSchemaId> {
        self.received_args_bindings
            .lock()
            .unwrap()
            .get(&method_id.0)
            .copied()
    }

    /// Look up the remote's root TypeSchemaId for a method's response.
    pub fn get_remote_response_root(&self, method_id: MethodId) -> Option<TypeSchemaId> {
        self.received_response_bindings
            .lock()
            .unwrap()
            .get(&method_id.0)
            .copied()
    }

    /// Look up a received schema by type ID.
    pub fn get_received(&self, type_id: &TypeSchemaId) -> Option<Schema> {
        self.received.lock().unwrap().get(type_id).cloned()
    }

    /// Get a snapshot of the received schema registry for building translation plans.
    pub fn received_registry(&self) -> SchemaRegistry {
        self.received.lock().unwrap().clone()
    }

    /// Extract all schemas for a type and its transitive dependencies.
    ///
    /// Returns schemas in dependency order: dependencies appear before dependents.
    /// The root type's schema is last.
    // r[impl schema.format]
    pub fn extract_schemas(&self, shape: &'static Shape) -> Vec<Schema> {
        let mut ctx = ExtractCtx {
            tracker: self,
            schemas: Vec::new(),
            seen: HashSet::new(),
            stack: Vec::new(),
        };
        ctx.extract(shape);
        ctx.schemas
    }
}

impl std::fmt::Debug for SchemaTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaTracker").finish_non_exhaustive()
    }
}

impl Default for SchemaTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract schemas without a tracker (uses a temporary counter).
/// Useful for tests and one-off schema extraction.
pub fn extract_schemas(shape: &'static Shape) -> Vec<Schema> {
    let tracker = SchemaTracker::new();
    tracker.extract_schemas(shape)
}

struct ExtractCtx<'a> {
    tracker: &'a SchemaTracker,
    schemas: Vec<Schema>,
    /// Shapes already fully processed.
    seen: HashSet<&'static Shape>,
    /// Stack for cycle detection.
    stack: Vec<&'static Shape>,
}

impl<'a> ExtractCtx<'a> {
    fn push_schema(&mut self, shape: &'static Shape, type_id: TypeSchemaId, kind: SchemaKind) {
        if self.seen.insert(shape) {
            self.schemas.push(Schema {
                type_id,
                name: format!("{shape}"),
                kind,
            });
        }
    }

    /// Extract a schema for the given shape, returning its TypeSchemaId.
    /// Recursively extracts dependencies first.
    fn extract(&mut self, shape: &'static Shape) -> TypeSchemaId {
        // Channel types: extract the element type, skip the channel wrapper.
        if (is_tx(shape) || is_rx(shape))
            && let Some(inner) = shape.type_params.first()
        {
            return self.extract(inner.shape);
        }

        // Transparent wrappers: follow inner.
        if shape.is_transparent()
            && let Some(inner) = shape.inner
        {
            return self.extract(inner);
        }

        let type_id = self.tracker.id_for_shape(shape);

        // Already fully processed — just return its id.
        if self.seen.contains(shape) {
            return type_id;
        }

        // r[impl schema.format.recursive]
        // Cycle detection: if on the stack, return the id without re-entering.
        if self.stack.contains(&shape) {
            return type_id;
        }

        // r[impl schema.format.primitive]
        // Scalars
        if let Some(scalar) = shape.scalar_type() {
            self.push_schema(
                shape,
                type_id,
                SchemaKind::Primitive {
                    primitive_type: scalar_to_primitive(scalar),
                },
            );
            return type_id;
        }

        // r[impl schema.format.container]
        // Containers
        match shape.def {
            Def::List(list_def) => {
                if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                    self.push_schema(
                        shape,
                        type_id,
                        SchemaKind::Primitive {
                            primitive_type: PrimitiveType::Bytes,
                        },
                    );
                } else {
                    let elem_id = self.extract(list_def.t());
                    self.push_schema(shape, type_id, SchemaKind::List { element: elem_id });
                }
                return type_id;
            }
            Def::Array(array_def) => {
                let elem_id = self.extract(array_def.t());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Array {
                        element: elem_id,
                        length: array_def.n as u64,
                    },
                );
                return type_id;
            }
            Def::Slice(slice_def) => {
                let elem_id = self.extract(slice_def.t());
                self.push_schema(shape, type_id, SchemaKind::List { element: elem_id });
                return type_id;
            }
            Def::Map(map_def) => {
                let key_id = self.extract(map_def.k());
                let val_id = self.extract(map_def.v());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Map {
                        key: key_id,
                        value: val_id,
                    },
                );
                return type_id;
            }
            Def::Set(set_def) => {
                let elem_id = self.extract(set_def.t());
                self.push_schema(shape, type_id, SchemaKind::Set { element: elem_id });
                return type_id;
            }
            Def::Option(opt_def) => {
                let elem_id = self.extract(opt_def.t());
                self.push_schema(shape, type_id, SchemaKind::Option { element: elem_id });
                return type_id;
            }
            Def::Result(result_def) => {
                let ok_id = self.extract(result_def.t());
                let err_id = self.extract(result_def.e());
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Enum {
                        variants: vec![
                            VariantSchema {
                                name: "Ok".to_string(),
                                index: 0,
                                payload: VariantPayload::Newtype { type_id: ok_id },
                            },
                            VariantSchema {
                                name: "Err".to_string(),
                                index: 1,
                                payload: VariantPayload::Newtype { type_id: err_id },
                            },
                        ],
                    },
                );
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
        self.stack.push(shape);

        let kind = match shape.ty {
            // r[impl schema.format.struct]
            // r[impl schema.format.tuple]
            Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
                StructKind::Unit => SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                },
                StructKind::TupleStruct | StructKind::Tuple => {
                    let elements: Vec<TypeSchemaId> = struct_type
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
                            required: f.default.is_none(),
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
                panic!("schema extraction: Pointer type without type_params: {shape}");
            }
            other => panic!(
                "schema extraction: unhandled type {other:?} for shape {shape} (def={:?})",
                shape.def
            ),
        };

        self.stack.pop();

        self.push_schema(shape, type_id, kind);

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

    // r[verify schema.type-id]
    #[test]
    fn type_ids_are_just_u32() {
        let id = TypeSchemaId(42);
        assert_eq!(id.0, 42);
        assert_eq!(id, TypeSchemaId(42));
        assert_ne!(id, TypeSchemaId(43));
    }

    // r[verify schema.principles.cbor]
    // r[verify schema.format.self-contained]
    // r[verify schema.format.batch]
    #[test]
    fn cbor_round_trip() {
        let schema = Schema {
            type_id: TypeSchemaId(1),
            name: "u32".into(),
            kind: SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32,
            },
        };
        let bytes = build_schema_message(std::slice::from_ref(&schema), &[]);
        let payload = parse_schema_message(&bytes).expect("should parse CBOR");
        assert_eq!(payload.schemas.len(), 1);
        assert_eq!(payload.schemas[0].type_id, schema.type_id);
    }

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
        let method = MethodId(1);
        let first =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(first.is_some(), "first prepare_send should return Some");
        let second =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
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
        let method = MethodId(1);
        let prepared = tracker
            .prepare_send_for_method(method, Outer::SHAPE, BindingDirection::Args)
            .expect("should return schemas");
        assert!(
            prepared.schemas.len() >= 3,
            "should include transitive deps, got {}",
            prepared.schemas.len()
        );

        // Same method again — nothing to send
        let again =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            again.is_none(),
            "u32 was already sent as transitive dep, method already bound"
        );
    }

    // r[verify schema.tracking.received]
    #[test]
    fn tracker_record_and_get_received() {
        let tracker = SchemaTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        let id = schemas[0].type_id;
        assert!(tracker.get_received(&id).is_none());
        tracker.record_received(SchemaMessagePayload {
            schemas,
            method_bindings: vec![],
        });
        assert!(tracker.get_received(&id).is_some());
    }

    // r[verify schema.type-id]
    #[test]
    fn type_ids_are_incrementing_u32() {
        let tracker = SchemaTracker::new();
        let schemas = tracker.extract_schemas(<(u32, String) as Facet>::SHAPE);
        // Should have u32, String, and the tuple — all with sequential IDs
        assert!(schemas.len() >= 3);
        // IDs should be small integers
        for s in &schemas {
            assert!(
                s.type_id.0 < 100,
                "expected small u32 ID, got {}",
                s.type_id.0
            );
        }
    }

    #[test]
    fn bidirectional_bindings_are_independent() {
        let tracker = SchemaTracker::new();
        let method = MethodId(1);

        // Send args binding
        let args = tracker
            .prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args)
            .expect("should send args");
        assert_eq!(args.method_bindings.len(), 1);
        assert_eq!(args.method_bindings[0].direction, BindingDirection::Args);

        // Send response binding for the same method — should NOT be deduplicated
        let response = tracker
            .prepare_send_for_method(method, <String as Facet>::SHAPE, BindingDirection::Response)
            .expect("should send response");
        assert_eq!(response.method_bindings.len(), 1);
        assert_eq!(
            response.method_bindings[0].direction,
            BindingDirection::Response
        );

        // Record received bindings and verify they go to separate maps
        tracker.record_received(SchemaMessagePayload {
            schemas: extract_schemas(<u64 as Facet>::SHAPE),
            method_bindings: vec![
                MethodSchemaBinding {
                    method_id: 42,
                    root_type_schema_id: TypeSchemaId(100),
                    direction: BindingDirection::Args,
                },
                MethodSchemaBinding {
                    method_id: 42,
                    root_type_schema_id: TypeSchemaId(200),
                    direction: BindingDirection::Response,
                },
            ],
        });

        assert_eq!(
            tracker.get_remote_args_root(MethodId(42)),
            Some(TypeSchemaId(100))
        );
        assert_eq!(
            tracker.get_remote_response_root(MethodId(42)),
            Some(TypeSchemaId(200))
        );
    }
}
