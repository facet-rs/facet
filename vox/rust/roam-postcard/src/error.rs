use std::fmt;

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
        }
    }
}

impl std::error::Error for DeserializeError {}

// r[impl schema.errors.content]
// r[impl schema.errors.early-detection]
#[derive(Debug)]
pub struct TranslationError {
    /// Path from the root type to the error site, e.g. `["inner", "name"]`.
    pub path: Vec<String>,
    /// Remote type ID of the root type being translated.
    pub remote_type_id: roam_schema::TypeId,
    /// Local type name for diagnostics.
    pub local_type_name: String,
    /// The specific incompatibility.
    pub kind: TranslationErrorKind,
}

#[derive(Debug)]
pub enum TranslationErrorKind {
    // r[impl schema.errors.missing-required]
    /// A required local field has no corresponding remote field and no default.
    MissingRequiredField {
        field_name: String,
        field_type: String,
    },
    // r[impl schema.errors.type-mismatch]
    /// A field exists in both types but the types are incompatible.
    TypeMismatch {
        field_name: String,
        remote_type: String,
        local_type: String,
    },
    /// Remote schema says "struct" but local type is something else (or vice versa).
    KindMismatch {
        remote_kind: String,
        local_kind: String,
    },
    /// Enum variant payloads are incompatible (e.g. unit vs struct).
    IncompatibleVariantPayload {
        variant_name: String,
        remote_payload: String,
        local_payload: String,
    },
    /// A type ID referenced by the remote schema was not found in the registry.
    SchemaNotFound { type_id: roam_schema::TypeId },
}

impl TranslationError {
    /// Push a path segment (field or variant name) onto the front of the path.
    /// Used when propagating errors up from nested plan building.
    pub fn with_path_prefix(mut self, segment: &str) -> Self {
        self.path.insert(0, segment.to_string());
        self
    }
}

impl fmt::Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = if self.path.is_empty() {
            self.local_type_name.clone()
        } else {
            format!("{}.{}", self.local_type_name, self.path.join("."))
        };

        write!(f, "at {path} (remote {:?}): ", self.remote_type_id)?;

        match &self.kind {
            TranslationErrorKind::MissingRequiredField {
                field_name,
                field_type,
            } => {
                write!(
                    f,
                    "missing required field '{field_name}' of type {field_type}"
                )
            }
            TranslationErrorKind::TypeMismatch {
                field_name,
                remote_type,
                local_type,
            } => write!(
                f,
                "type mismatch on field '{field_name}': remote has {remote_type}, local has {local_type}"
            ),
            TranslationErrorKind::KindMismatch {
                remote_kind,
                local_kind,
            } => write!(
                f,
                "structural mismatch: remote is {remote_kind}, local is {local_kind}"
            ),
            TranslationErrorKind::IncompatibleVariantPayload {
                variant_name,
                remote_payload,
                local_payload,
            } => write!(
                f,
                "variant '{variant_name}' payload mismatch: remote={remote_payload}, local={local_payload}"
            ),
            TranslationErrorKind::SchemaNotFound { type_id } => {
                write!(f, "schema not found for type ID {type_id:?}")
            }
        }
    }
}

impl std::error::Error for TranslationError {}
