use std::fmt;

use vox_schema::{
    ChannelDirection, FieldSchema, PrimitiveType, Schema, SchemaHash, SchemaKind, SchemaRegistry,
    TypeRef, VariantSchema,
};

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
    NameMismatch {
        remote: Schema,
        local: Schema,
        remote_rust: String,
        local_rust: String,
    },
    /// The structural kinds don't match (e.g. remote is enum, local is struct).
    KindMismatch {
        remote: Schema,
        local: Schema,
        remote_rust: String,
        local_rust: String,
    },
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
        type_id: SchemaHash,
        /// Which side was missing it.
        side: SchemaSide,
    },
    /// Tuple lengths don't match.
    TupleLengthMismatch {
        remote: Schema,
        local: Schema,
        remote_rust: String,
        local_rust: String,
        remote_len: usize,
        local_len: usize,
    },
    /// A type variable (Var) appeared where a concrete type was expected.
    /// This means Var substitution didn't happen — a bug in the extraction
    /// or plan building pipeline.
    UnresolvedVar { name: String, side: SchemaSide },
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

pub(crate) fn format_schema_rust(schema: &Schema, registry: &SchemaRegistry) -> String {
    match &schema.kind {
        SchemaKind::Struct { name, .. } | SchemaKind::Enum { name, .. } => name.clone(),
        kind => format_schema_kind_rust(kind, registry),
    }
}

pub(crate) fn format_type_ref_rust(type_ref: &TypeRef, registry: &SchemaRegistry) -> String {
    match type_ref {
        TypeRef::Var { name } => name.as_str().to_string(),
        TypeRef::Concrete { type_id, args } => {
            let Some(schema) = registry.get(type_id) else {
                return format!("<missing:{type_id:?}>");
            };
            match &schema.kind {
                SchemaKind::Struct { name, .. } | SchemaKind::Enum { name, .. } => {
                    if args.is_empty() {
                        name.clone()
                    } else {
                        let args = args
                            .iter()
                            .map(|arg| format_type_ref_rust(arg, registry))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{name}<{args}>")
                    }
                }
                kind => format_schema_kind_rust(kind, registry),
            }
        }
    }
}

fn format_schema_kind_rust(kind: &SchemaKind, registry: &SchemaRegistry) -> String {
    match kind {
        SchemaKind::Struct { name, .. } | SchemaKind::Enum { name, .. } => name.clone(),
        SchemaKind::Tuple { elements } => {
            let elements = elements
                .iter()
                .map(|element| format_type_ref_rust(element, registry))
                .collect::<Vec<_>>();
            match elements.len() {
                0 => "()".to_string(),
                1 => format!("({},)", elements[0]),
                _ => format!("({})", elements.join(", ")),
            }
        }
        SchemaKind::List { element } => format!("Vec<{}>", format_type_ref_rust(element, registry)),
        SchemaKind::Map { key, value } => format!(
            "HashMap<{}, {}>",
            format_type_ref_rust(key, registry),
            format_type_ref_rust(value, registry)
        ),
        SchemaKind::Array { element, length } => {
            format!("[{}; {length}]", format_type_ref_rust(element, registry))
        }
        SchemaKind::Option { element } => {
            format!("Option<{}>", format_type_ref_rust(element, registry))
        }
        SchemaKind::Channel { direction, element } => format!(
            "{}<{}>",
            match direction {
                ChannelDirection::Tx => "Tx",
                ChannelDirection::Rx => "Rx",
            },
            format_type_ref_rust(element, registry)
        ),
        SchemaKind::Primitive { primitive_type } => match primitive_type {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::U8 => "u8".to_string(),
            PrimitiveType::U16 => "u16".to_string(),
            PrimitiveType::U32 => "u32".to_string(),
            PrimitiveType::U64 => "u64".to_string(),
            PrimitiveType::U128 => "u128".to_string(),
            PrimitiveType::I8 => "i8".to_string(),
            PrimitiveType::I16 => "i16".to_string(),
            PrimitiveType::I32 => "i32".to_string(),
            PrimitiveType::I64 => "i64".to_string(),
            PrimitiveType::I128 => "i128".to_string(),
            PrimitiveType::F32 => "f32".to_string(),
            PrimitiveType::F64 => "f64".to_string(),
            PrimitiveType::Char => "char".to_string(),
            PrimitiveType::String => "String".to_string(),
            PrimitiveType::Unit => "()".to_string(),
            PrimitiveType::Never => "never".to_string(),
            PrimitiveType::Bytes => "Vec<u8>".to_string(),
            PrimitiveType::Payload => "Payload".to_string(),
        },
    }
}

impl fmt::Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.path.segments.is_empty() {
            write!(f, "at {}: ", self.path)?;
        }

        match &*self.kind {
            TranslationErrorKind::NameMismatch {
                remote,
                local,
                remote_rust,
                local_rust,
            } => {
                write!(
                    f,
                    "type name mismatch: remote is `{remote_rust}`, local is `{local_rust}` (remote name `{}`, local name `{}`)",
                    remote.name().unwrap_or("<anonymous>"),
                    local.name().unwrap_or("<anonymous>"),
                )
            }
            TranslationErrorKind::KindMismatch {
                remote_rust,
                local_rust,
                ..
            } => {
                write!(
                    f,
                    "structural mismatch: remote is `{remote_rust}`, local is `{local_rust}`",
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
                    format_schema_rust(remote_struct, &SchemaRegistry::new()),
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
                    format_schema_rust(remote_field_type, &SchemaRegistry::new()),
                    format_schema_rust(local_field_type, &SchemaRegistry::new()),
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
                remote_rust,
                local_rust,
                remote_len,
                local_len,
                ..
            } => {
                write!(
                    f,
                    "tuple length mismatch: remote `{remote_rust}` has {remote_len} elements, local `{local_rust}` has {local_len} elements",
                )
            }
            TranslationErrorKind::UnresolvedVar { name, side } => {
                write!(
                    f,
                    "unresolved type variable {name} on {side} side — Var substitution failed"
                )
            }
        }
    }
}

fn variant_payload_str(payload: &vox_schema::VariantPayload) -> &'static str {
    match payload {
        vox_schema::VariantPayload::Unit => "unit",
        vox_schema::VariantPayload::Newtype { .. } => "newtype",
        vox_schema::VariantPayload::Tuple { .. } => "tuple",
        vox_schema::VariantPayload::Struct { .. } => "struct",
    }
}

impl std::error::Error for TranslationError {}
