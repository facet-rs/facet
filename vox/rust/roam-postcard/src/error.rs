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

#[derive(Debug)]
pub enum TranslationError {
    MissingRequiredField {
        name: String,
    },
    TypeMismatch {
        field: String,
        remote: String,
        local: String,
    },
    UnknownVariant {
        name: String,
    },
    IncompatibleVariantPayload {
        variant: String,
    },
    SchemaNotFound {
        type_id: roam_schema::TypeId,
    },
}

impl fmt::Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequiredField { name } => {
                write!(f, "missing required field: {name}")
            }
            Self::TypeMismatch {
                field,
                remote,
                local,
            } => write!(
                f,
                "type mismatch for field '{field}': remote={remote}, local={local}"
            ),
            Self::UnknownVariant { name } => write!(f, "unknown variant: {name}"),
            Self::IncompatibleVariantPayload { variant } => {
                write!(f, "incompatible variant payload: {variant}")
            }
            Self::SchemaNotFound { type_id } => {
                write!(f, "schema not found for type_id {type_id:?}")
            }
        }
    }
}

impl std::error::Error for TranslationError {}
