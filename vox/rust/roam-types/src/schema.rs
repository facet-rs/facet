//! Schema types, extraction, and tracking for roam wire protocol.
//!
//! This module contains:
//! - Schema data types (TypeSchemaId, Schema, SchemaKind, etc.)
//! - CBOR serialization for schema messages
//! - Schema extraction from facet Shape graphs
//! - SchemaSendTracker for outbound dedup (owned by SessionCore)
//! - SchemaRecvTracker for inbound storage (shared via Arc)

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
    pub kind: SchemaKind,
}

impl Schema {
    /// Returns the type name for nominal types (struct/enum), or `None` for
    /// structural types (tuple, list, map, etc.).
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            SchemaKind::Struct { name, .. } | SchemaKind::Enum { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }
}

/// The structural kind of a type.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SchemaKind {
    Struct {
        /// The type name (e.g. "Point"). Used for matching across schema
        /// versions and for diagnostics. MUST NOT be empty.
        name: String,
        fields: Vec<FieldSchema>,
    },
    Enum {
        /// The type name (e.g. "Color"). Used for matching across schema
        /// versions and for diagnostics. MUST NOT be empty.
        name: String,
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
    Tuple { types: Vec<TypeSchemaId> },
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
    /// An opaque payload — a length-prefixed byte sequence whose
    /// length prefix is a little-endian u32 (not a varint like other
    /// postcard sequences).
    Payload,
}

/// CBOR-encoded schema payload (schemas + method bindings).
///
/// Newtype over `Vec<u8>` so the type system distinguishes raw bytes from
/// CBOR-encoded schema data. Empty when no new schemas need to be sent.
#[derive(Facet, Clone, Debug, Default)]
#[repr(transparent)]
#[facet(transparent)]
pub struct CborPayload(pub Vec<u8>);

impl CborPayload {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Parse the CBOR-encoded schema message payload.
    pub fn parse(&self) -> Result<SchemaPayload, facet_cbor::CborError> {
        parse_schema_message(&self.0)
    }
}

/// Lookup table mapping TypeSchemaId → Schema, used for resolving type
/// references during deserialization with translation plans.
pub type SchemaRegistry = HashMap<TypeSchemaId, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.type_id, s.clone())).collect()
}

/// Binds a method to the root TypeSchemaId of the type being sent for that
/// method. Sent once per method per direction.
#[derive(Facet, Clone, Debug)]
pub struct MethodSchemaBinding {
    pub method_id: MethodId,
    pub root_type_schema_id: TypeSchemaId,
    /// Whether this binding is for args (caller → callee) or response (callee → caller).
    pub direction: BindingDirection,
}

/// Whether a method schema binding describes args or the response type.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
pub struct SchemaPayload {
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
    let payload = SchemaPayload {
        schemas: schemas.to_vec(),
        method_bindings: method_bindings.to_vec(),
    };
    facet_cbor::to_vec(&payload).expect("schema CBOR serialization should not fail")
}

/// Parse a CBOR-encoded schema message.
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn parse_schema_message(bytes: &[u8]) -> Result<SchemaPayload, facet_cbor::CborError> {
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

impl PreparedSchemaMessage {
    /// CBOR-encode this prepared message for embedding in RequestCall/RequestResponse.
    pub fn to_cbor(&self) -> CborPayload {
        CborPayload(build_schema_message(&self.schemas, &self.method_bindings))
    }
}

// ============================================================================
// SchemaSendTracker — outbound dedup, owned by SessionCore (no Arc, no Mutex)
// ============================================================================

/// Tracks which schemas have been sent on the current connection.
///
/// Plain struct — owned by `SessionCore` behind the same Mutex as the
/// conduit tx. Reset on reconnection.
// r[impl schema.tracking.sent]
// r[impl schema.type-id.per-connection]
pub struct SchemaSendTracker {
    /// Per-method, per-direction: the CborPayload that was sent. Keyed by
    /// (method_id, direction). If present, schemas were already sent.
    sent_methods: HashMap<(MethodId, BindingDirection), CborPayload>,
    /// Assigns incrementing type IDs to shapes we extract.
    shape_to_id: HashMap<&'static Shape, TypeSchemaId>,
    /// All type schema IDs we've sent so far (for dedup across methods that
    /// share types).
    sent_type_ids: HashSet<TypeSchemaId>,
    /// Next ID to assign.
    next_id: u32,
}

impl SchemaSendTracker {
    pub fn new() -> Self {
        SchemaSendTracker {
            sent_methods: HashMap::new(),
            shape_to_id: HashMap::new(),
            sent_type_ids: HashSet::new(),
            next_id: 1,
        }
    }

    /// Reset all state — call on reconnection.
    pub fn reset(&mut self) {
        self.sent_methods.clear();
        self.shape_to_id.clear();
        self.sent_type_ids.clear();
        self.next_id = 1;
    }

    /// Allocate a fresh TypeSchemaId not associated with any Shape.
    /// Used for synthetic schemas (e.g., args tuple wrappers) created during codegen.
    pub fn allocate_anonymous_id(&mut self) -> TypeSchemaId {
        let id = TypeSchemaId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Allocate or look up a TypeSchemaId for a Shape.
    fn id_for_shape(&mut self, shape: &'static Shape) -> TypeSchemaId {
        if let Some(&id) = self.shape_to_id.get(shape) {
            return id;
        }
        let id = TypeSchemaId(self.next_id);
        self.next_id += 1;
        self.shape_to_id.insert(shape, id);
        id
    }

    /// Prepare schemas for a method call/response, returning a CBOR payload
    /// to inline in the request/response. Returns empty payload if schemas
    /// were already sent for this method+direction.
    ///
    /// Fast path: if method+direction is in `sent_methods`, return immediately.
    /// Slow path: extract schemas, deduplicate, CBOR-encode, cache, return.
    // r[impl schema.tracking.transitive]
    // r[impl schema.exchange.idempotent]
    // r[impl schema.principles.once-per-type]
    // r[impl schema.principles.sender-driven]
    // r[impl schema.principles.no-roundtrips]
    pub fn prepare_send_for_method(
        &mut self,
        method_id: MethodId,
        shape: &'static Shape,
        direction: BindingDirection,
    ) -> CborPayload {
        let key = (method_id, direction);

        // Fast path: already sent for this method+direction.
        if self.sent_methods.contains_key(&key) {
            return CborPayload::default();
        }

        // Slow path: extract, deduplicate, encode.
        let all_schemas = self.extract_schemas(shape);
        let root_type_schema_id = match all_schemas.last() {
            Some(s) => s.type_id,
            None => return CborPayload::default(),
        };

        let unsent: Vec<Schema> = all_schemas
            .into_iter()
            .filter(|s| self.sent_type_ids.insert(s.type_id))
            .collect();

        let method_binding = MethodSchemaBinding {
            method_id,
            root_type_schema_id,
            direction,
        };

        let prepared = PreparedSchemaMessage {
            schemas: unsent,
            method_bindings: vec![method_binding],
        };
        let cbor = prepared.to_cbor();
        self.sent_methods.insert(key, cbor.clone());
        cbor
    }

    /// Extract all schemas for a type and its transitive dependencies.
    ///
    /// Returns schemas in dependency order: dependencies appear before dependents.
    /// The root type's schema is last.
    // r[impl schema.format]
    pub fn extract_schemas(&mut self, shape: &'static Shape) -> Vec<Schema> {
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

impl Default for SchemaSendTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SchemaSendTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaSendTracker").finish_non_exhaustive()
    }
}

// ============================================================================
// SchemaRecvTracker — inbound storage, shared via Arc
// ============================================================================

/// Tracks schemas received from the remote peer on the current connection.
///
/// Uses interior mutability (Mutex) so it can be shared via `Arc` between the
/// session recv loop and in-flight handler tasks. Created fresh on each
/// connection — NOT reused across reconnections.
// r[impl schema.tracking.received]
// r[impl schema.type-id.per-connection]
pub struct SchemaRecvTracker {
    /// Type schemas received from the remote peer.
    received: Mutex<HashMap<TypeSchemaId, Schema>>,
    /// Args bindings received: method_id → root TypeSchemaId for args.
    received_args_bindings: Mutex<HashMap<MethodId, TypeSchemaId>>,
    /// Response bindings received: method_id → root TypeSchemaId for response.
    received_response_bindings: Mutex<HashMap<MethodId, TypeSchemaId>>,
}

/// Error returned when recording received schemas detects a protocol violation.
#[derive(Debug)]
pub struct DuplicateSchemaError {
    pub type_id: TypeSchemaId,
}

impl std::fmt::Display for DuplicateSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "duplicate TypeSchemaId {:?} received on same connection — protocol error",
            self.type_id
        )
    }
}

impl std::error::Error for DuplicateSchemaError {}

impl SchemaRecvTracker {
    pub fn new() -> Self {
        SchemaRecvTracker {
            received: Mutex::new(HashMap::new()),
            received_args_bindings: Mutex::new(HashMap::new()),
            received_response_bindings: Mutex::new(HashMap::new()),
        }
    }

    /// Record a parsed schema message from the remote peer.
    ///
    /// Returns `Err` if a TypeSchemaId was already received — this is a
    /// protocol error (the send tracker didn't reset on reconnection).
    pub fn record_received(&self, payload: SchemaPayload) -> Result<(), DuplicateSchemaError> {
        {
            let mut received = self.received.lock().unwrap();
            for schema in payload.schemas {
                if received.contains_key(&schema.type_id) {
                    return Err(DuplicateSchemaError {
                        type_id: schema.type_id,
                    });
                }
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
        Ok(())
    }

    /// Look up the remote's root TypeSchemaId for a method's args.
    pub fn get_remote_args_root(&self, method_id: MethodId) -> Option<TypeSchemaId> {
        self.received_args_bindings
            .lock()
            .unwrap()
            .get(&method_id)
            .copied()
    }

    /// Look up the remote's root TypeSchemaId for a method's response.
    pub fn get_remote_response_root(&self, method_id: MethodId) -> Option<TypeSchemaId> {
        self.received_response_bindings
            .lock()
            .unwrap()
            .get(&method_id)
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
}

impl Default for SchemaRecvTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SchemaRecvTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRecvTracker").finish_non_exhaustive()
    }
}

/// Extract schemas without a tracker (uses a temporary counter).
/// Useful for tests and one-off schema extraction.
pub fn extract_schemas(shape: &'static Shape) -> Vec<Schema> {
    let mut tracker = SchemaSendTracker::new();
    tracker.extract_schemas(shape)
}

struct ExtractCtx<'a> {
    tracker: &'a mut SchemaSendTracker,
    schemas: Vec<Schema>,
    /// Shapes already fully processed.
    seen: HashSet<&'static Shape>,
    /// Stack for cycle detection.
    stack: Vec<&'static Shape>,
}

impl<'a> ExtractCtx<'a> {
    fn push_schema(&mut self, shape: &'static Shape, type_id: TypeSchemaId, kind: SchemaKind) {
        if self.seen.insert(shape) {
            self.schemas.push(Schema { type_id, kind });
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
                self.push_schema(shape, type_id, SchemaKind::List { element: elem_id });
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
                        name: format!("{shape}"),
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
                    SchemaKind::Struct {
                        name: format!("{shape}"),
                        fields,
                    }
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
                                    let types: Vec<TypeSchemaId> = v
                                        .data
                                        .fields
                                        .iter()
                                        .map(|f| self.extract(f.shape()))
                                        .collect();
                                    VariantPayload::Tuple { types }
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
                SchemaKind::Enum {
                    name: format!("{shape}"),
                    variants,
                }
            }
            Type::User(UserType::Opaque) => {
                // Opaque types (like Payload) are represented as bytes on the wire.
                self.stack.pop();
                self.push_schema(
                    shape,
                    type_id,
                    SchemaKind::Primitive {
                        primitive_type: PrimitiveType::Bytes,
                    },
                );
                return type_id;
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
            SchemaKind::Struct { name, fields } => {
                assert!(
                    name.contains("Point"),
                    "expected name to contain Point, got {name}"
                );
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
            SchemaKind::Enum { variants, .. } => {
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
            SchemaKind::Enum { variants, .. } => {
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
    fn tracker_prepare_send_returns_payload_then_empty() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let first =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            !first.is_empty(),
            "first prepare_send should return payload"
        );
        let second =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            second.is_empty(),
            "second prepare_send for same method should return empty"
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

        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let first = tracker.prepare_send_for_method(method, Outer::SHAPE, BindingDirection::Args);
        assert!(!first.is_empty(), "should return schemas");
        let parsed = first.parse().expect("should parse CBOR");
        assert!(
            parsed.schemas.len() >= 3,
            "should include transitive deps, got {}",
            parsed.schemas.len()
        );

        // Same method again — nothing to send
        let again =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            again.is_empty(),
            "u32 was already sent as transitive dep, method already bound"
        );
    }

    // r[verify schema.tracking.received]
    #[test]
    fn tracker_record_and_get_received() {
        let tracker = SchemaRecvTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        let id = schemas[0].type_id;
        assert!(tracker.get_received(&id).is_none());
        tracker
            .record_received(SchemaPayload {
                schemas,
                method_bindings: vec![],
            })
            .expect("first record should succeed");
        assert!(tracker.get_received(&id).is_some());
    }

    // r[verify schema.type-id]
    #[test]
    fn type_ids_are_incrementing_u32() {
        let mut tracker = SchemaSendTracker::new();
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
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);

        // Send args binding
        let args =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(!args.is_empty(), "should send args");
        let args_parsed = args.parse().expect("parse args CBOR");
        assert_eq!(args_parsed.method_bindings.len(), 1);
        assert_eq!(
            args_parsed.method_bindings[0].direction,
            BindingDirection::Args
        );

        // Send response binding for the same method — should NOT be deduplicated
        let response = tracker.prepare_send_for_method(
            method,
            <String as Facet>::SHAPE,
            BindingDirection::Response,
        );
        assert!(!response.is_empty(), "should send response");
        let response_parsed = response.parse().expect("parse response CBOR");
        assert_eq!(response_parsed.method_bindings.len(), 1);
        assert_eq!(
            response_parsed.method_bindings[0].direction,
            BindingDirection::Response
        );

        // Record received bindings and verify they go to separate maps
        let recv_tracker = SchemaRecvTracker::new();
        recv_tracker
            .record_received(SchemaPayload {
                schemas: extract_schemas(<u64 as Facet>::SHAPE),
                method_bindings: vec![
                    MethodSchemaBinding {
                        method_id: MethodId(42),
                        root_type_schema_id: TypeSchemaId(100),
                        direction: BindingDirection::Args,
                    },
                    MethodSchemaBinding {
                        method_id: MethodId(42),
                        root_type_schema_id: TypeSchemaId(200),
                        direction: BindingDirection::Response,
                    },
                ],
            })
            .expect("record should succeed");

        assert_eq!(
            recv_tracker.get_remote_args_root(MethodId(42)),
            Some(TypeSchemaId(100))
        );
        assert_eq!(
            recv_tracker.get_remote_response_root(MethodId(42)),
            Some(TypeSchemaId(200))
        );
    }

    #[test]
    fn duplicate_schema_is_protocol_error() {
        let tracker = SchemaRecvTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        tracker
            .record_received(SchemaPayload {
                schemas: schemas.clone(),
                method_bindings: vec![],
            })
            .expect("first record should succeed");
        let err = tracker
            .record_received(SchemaPayload {
                schemas,
                method_bindings: vec![],
            })
            .expect_err("duplicate should fail");
        assert_eq!(err.type_id, TypeSchemaId(1));
    }

    #[test]
    fn send_tracker_reset_clears_all_state() {
        let mut tracker = SchemaSendTracker::new();
        let method = MethodId(1);
        let first =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(!first.is_empty(), "first should return payload");

        tracker.reset();

        let after_reset =
            tracker.prepare_send_for_method(method, <u32 as Facet>::SHAPE, BindingDirection::Args);
        assert!(
            !after_reset.is_empty(),
            "after reset, prepare_send should return payload again"
        );
    }
}
