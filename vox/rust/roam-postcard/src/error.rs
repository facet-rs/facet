use std::fmt;

use roam_types::{FieldSchema, Schema, SchemaKind, TypeSchemaId, VariantSchema};

#[derive(Debug)]
pub enum SerializeError {
    UnsupportedType(String),
    ReflectError(String),
}

impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(ty) => write!(f, "unsupported type: {ty}"),
            Self::ReflectError(msg) => write!(f, "reflect error: {msg}"),
        }
    }
}

impl std::error::Error for SerializeError {}

#[derive(Debug)]
pub enum DeserializeError {
    UnexpectedEof {
        pos: usize,
    },
    VarintOverflow {
        pos: usize,
    },
    InvalidBool {
        pos: usize,
        got: u8,
    },
    InvalidUtf8 {
        pos: usize,
    },
    InvalidOptionTag {
        pos: usize,
        got: u8,
    },
    InvalidEnumDiscriminant {
        pos: usize,
        index: u64,
        variant_count: usize,
    },
    UnsupportedType(String),
    ReflectError(String),
    UnknownVariant {
        remote_index: usize,
    },
    TrailingBytes {
        pos: usize,
        len: usize,
    },
    Custom(String),
    /// A protocol-level error: missing schemas, missing tracker, etc.
    // r[impl schema.exchange.required]
    Protocol(String),
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { pos } => write!(f, "unexpected EOF at byte {pos}"),
            Self::VarintOverflow { pos } => write!(f, "varint overflow at byte {pos}"),
            Self::InvalidBool { pos, got } => write!(f, "invalid bool 0x{got:02x} at byte {pos}"),
            Self::InvalidUtf8 { pos } => write!(f, "invalid UTF-8 at byte {pos}"),
            Self::InvalidOptionTag { pos, got } => {
                write!(f, "invalid option tag 0x{got:02x} at byte {pos}")
            }
            Self::InvalidEnumDiscriminant {
                pos,
                index,
                variant_count,
            } => {
                write!(
                    f,
                    "enum discriminant {index} out of range (0..{variant_count}) at byte {pos}"
                )
            }
            Self::UnsupportedType(ty) => write!(f, "unsupported type: {ty}"),
            Self::ReflectError(msg) => write!(f, "reflect error: {msg}"),
            Self::UnknownVariant { remote_index } => {
                write!(f, "unknown remote enum variant index {remote_index}")
            }
            Self::TrailingBytes { pos, len } => {
                write!(
                    f,
                    "trailing bytes: {remaining} at byte {pos}",
                    remaining = len - pos
                )
            }
            Self::Custom(msg) => write!(f, "{msg}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl DeserializeError {
    pub fn protocol(msg: &str) -> Self {
        Self::Protocol(msg.to_string())
    }
}

impl std::error::Error for DeserializeError {}

/// Path from a root type to a specific location in the schema tree.
///
/// Formatted as `RootType.field.nested_field` or `RootType::Variant.field`.
#[derive(Debug, Clone, Default)]
pub struct SchemaPath {
    segments: Vec<PathSegment>,
}

/// One segment in a schema path.
#[derive(Debug, Clone)]
pub enum PathSegment {
    /// A struct field: `.field_name`
    Field(String),
    /// An enum variant: `::VariantName`
    Variant(String),
    /// A tuple element: `.0`, `.1`, etc.
    Index(usize),
}

impl SchemaPath {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Prepend a segment to the front of the path.
    pub fn push_front(&mut self, segment: PathSegment) {
        self.segments.insert(0, segment);
    }

    pub fn with_front(mut self, segment: PathSegment) -> Self {
        self.push_front(segment);
        self
    }
}

impl fmt::Display for SchemaPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.segments {
            match segment {
                PathSegment::Field(name) => write!(f, ".{name}")?,
                PathSegment::Variant(name) => write!(f, "::{name}")?,
                PathSegment::Index(i) => write!(f, ".{i}")?,
            }
        }
        Ok(())
    }
}

// r[impl schema.errors.content]
// r[impl schema.errors.early-detection]
#[derive(Debug)]
pub struct TranslationError {
    /// Path from the root type to the error site.
    pub path: SchemaPath,
    /// The specific incompatibility.
    pub kind: Box<TranslationErrorKind>,
}

#[derive(Debug)]
pub enum TranslationErrorKind {
    /// The root type names don't match.
    NameMismatch { remote: Schema, local: Schema },
    /// The structural kinds don't match (e.g. remote is enum, local is struct).
    KindMismatch { remote: Schema, local: Schema },
    // r[impl schema.errors.missing-required]
    /// A required local field has no corresponding remote field and no default.
    MissingRequiredField {
        /// The local field that's missing from the remote schema.
        field: FieldSchema,
        /// The remote struct schema (so you can see what fields it does have).
        remote_struct: Schema,
    },
    // r[impl schema.errors.type-mismatch]
    /// A field exists in both types but the nested types are incompatible.
    FieldTypeMismatch {
        field_name: String,
        remote_field_type: Schema,
        local_field_type: Schema,
        /// The nested error that explains what exactly is incompatible.
        source: Box<TranslationError>,
    },
    /// Enum variant payloads are incompatible (e.g. unit vs struct).
    IncompatibleVariantPayload {
        remote_variant: VariantSchema,
        local_variant: VariantSchema,
    },
    /// A type ID referenced by the remote schema was not found in the registry.
    SchemaNotFound {
        type_id: TypeSchemaId,
        /// Which side was missing it.
        side: SchemaSide,
    },
    /// Tuple lengths don't match.
    TupleLengthMismatch {
        remote: Schema,
        local: Schema,
        remote_len: usize,
        local_len: usize,
    },
}

/// Which side of the schema comparison a missing schema was on.
#[derive(Debug, Clone, Copy)]
pub enum SchemaSide {
    Remote,
    Local,
}

impl fmt::Display for SchemaSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaSide::Remote => write!(f, "remote"),
            SchemaSide::Local => write!(f, "local"),
        }
    }
}

impl TranslationError {
    pub fn new(kind: TranslationErrorKind) -> Self {
        Self {
            path: SchemaPath::new(),
            kind: Box::new(kind),
        }
    }

    /// Prepend a path segment when propagating errors up from nested plan building.
    pub fn with_path_prefix(mut self, segment: PathSegment) -> Self {
        self.path.push_front(segment);
        self
    }
}

fn schema_label(schema: &Schema) -> &str {
    match &schema.kind {
        SchemaKind::Struct { name, .. } | SchemaKind::Enum { name, .. } => name.as_str(),
        other => schema_kind_str(other),
    }
}

fn schema_kind_str(kind: &SchemaKind) -> &'static str {
    match kind {
        SchemaKind::Struct { .. } => "struct",
        SchemaKind::Enum { .. } => "enum",
        SchemaKind::Tuple { .. } => "tuple",
        SchemaKind::List { .. } => "list",
        SchemaKind::Map { .. } => "map",
        SchemaKind::Array { .. } => "array",
        SchemaKind::Option { .. } => "option",
        SchemaKind::Primitive { .. } => "primitive",
        SchemaKind::Channel { .. } => "channel",
    }
}

impl fmt::Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.path.segments.is_empty() {
            write!(f, "at {}: ", self.path)?;
        }

        match &*self.kind {
            TranslationErrorKind::NameMismatch { remote, local } => {
                write!(
                    f,
                    "type name mismatch: remote is '{}' ({}), local is '{}' ({})",
                    schema_label(remote),
                    schema_kind_str(&remote.kind),
                    schema_label(local),
                    schema_kind_str(&local.kind),
                )
            }
            TranslationErrorKind::KindMismatch { remote, local } => {
                write!(
                    f,
                    "structural mismatch: remote '{}' is {}, local '{}' is {}",
                    schema_label(remote),
                    schema_kind_str(&remote.kind),
                    schema_label(local),
                    schema_kind_str(&local.kind),
                )
            }
            TranslationErrorKind::MissingRequiredField {
                field,
                remote_struct,
            } => {
                write!(
                    f,
                    "required field '{}' (type {:?}) missing from remote '{}'",
                    field.name,
                    field.type_ref,
                    schema_label(remote_struct),
                )?;
                if let SchemaKind::Struct { fields, .. } = &remote_struct.kind {
                    write!(f, " (remote has fields: ")?;
                    for (i, rf) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", rf.name)?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            TranslationErrorKind::FieldTypeMismatch {
                field_name,
                remote_field_type,
                local_field_type,
                source,
            } => {
                write!(
                    f,
                    "field '{field_name}' type mismatch: remote is '{}', local is '{}': {source}",
                    schema_label(remote_field_type),
                    schema_label(local_field_type),
                )
            }
            TranslationErrorKind::IncompatibleVariantPayload {
                remote_variant,
                local_variant,
            } => {
                write!(
                    f,
                    "variant '{}' payload mismatch: remote is {}, local is {}",
                    remote_variant.name,
                    variant_payload_str(&remote_variant.payload),
                    variant_payload_str(&local_variant.payload),
                )
            }
            TranslationErrorKind::SchemaNotFound { type_id, side } => {
                write!(f, "{side} schema not found for type ID {type_id:?}")
            }
            TranslationErrorKind::TupleLengthMismatch {
                remote,
                local,
                remote_len,
                local_len,
            } => {
                write!(
                    f,
                    "tuple length mismatch: remote '{}' has {remote_len} elements, local '{}' has {local_len} elements",
                    schema_label(remote),
                    schema_label(local),
                )
            }
        }
    }
}

fn variant_payload_str(payload: &roam_types::VariantPayload) -> &'static str {
    match payload {
        roam_types::VariantPayload::Unit => "unit",
        roam_types::VariantPayload::Newtype { .. } => "newtype",
        roam_types::VariantPayload::Tuple { .. } => "tuple",
        roam_types::VariantPayload::Struct { .. } => "struct",
    }
}

impl std::error::Error for TranslationError {}
