//! Error codes and error types.

use core::fmt;

/// RPC error codes.
///
/// Codes 0-99 align with gRPC for familiarity.
/// Codes 100+ are rapace-specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ErrorCode {
    // gRPC-aligned (0-99)
    Ok = 0,
    Cancelled = 1,
    DeadlineExceeded = 2,
    InvalidArgument = 3,
    NotFound = 4,
    AlreadyExists = 5,
    PermissionDenied = 6,
    ResourceExhausted = 7,
    FailedPrecondition = 8,
    Aborted = 9,
    OutOfRange = 10,
    Unimplemented = 11,
    Internal = 12,
    Unavailable = 13,
    DataLoss = 14,

    // rapace-specific (100+)
    PeerDied = 100,
    SessionClosed = 101,
    ValidationFailed = 102,
    StaleGeneration = 103,
}

impl ErrorCode {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Ok),
            1 => Some(Self::Cancelled),
            2 => Some(Self::DeadlineExceeded),
            3 => Some(Self::InvalidArgument),
            4 => Some(Self::NotFound),
            5 => Some(Self::AlreadyExists),
            6 => Some(Self::PermissionDenied),
            7 => Some(Self::ResourceExhausted),
            8 => Some(Self::FailedPrecondition),
            9 => Some(Self::Aborted),
            10 => Some(Self::OutOfRange),
            11 => Some(Self::Unimplemented),
            12 => Some(Self::Internal),
            13 => Some(Self::Unavailable),
            14 => Some(Self::DataLoss),
            100 => Some(Self::PeerDied),
            101 => Some(Self::SessionClosed),
            102 => Some(Self::ValidationFailed),
            103 => Some(Self::StaleGeneration),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => write!(f, "ok"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::DeadlineExceeded => write!(f, "deadline exceeded"),
            Self::InvalidArgument => write!(f, "invalid argument"),
            Self::NotFound => write!(f, "not found"),
            Self::AlreadyExists => write!(f, "already exists"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::ResourceExhausted => write!(f, "resource exhausted"),
            Self::FailedPrecondition => write!(f, "failed precondition"),
            Self::Aborted => write!(f, "aborted"),
            Self::OutOfRange => write!(f, "out of range"),
            Self::Unimplemented => write!(f, "unimplemented"),
            Self::Internal => write!(f, "internal error"),
            Self::Unavailable => write!(f, "unavailable"),
            Self::DataLoss => write!(f, "data loss"),
            Self::PeerDied => write!(f, "peer died"),
            Self::SessionClosed => write!(f, "session closed"),
            Self::ValidationFailed => write!(f, "validation failed"),
            Self::StaleGeneration => write!(f, "stale generation"),
        }
    }
}

/// Transport-level errors.
#[derive(Debug)]
pub enum TransportError {
    Closed,
    Io(std::io::Error),
    Validation(ValidationError),
    Encode(EncodeError),
    Decode(DecodeError),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "transport closed"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Validation(e) => write!(f, "validation error: {e}"),
            Self::Encode(e) => write!(f, "serialize error: {e}"),
            Self::Decode(e) => write!(f, "deserialize error: {e}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Validation(e) => Some(e),
            Self::Encode(e) => Some(e),
            Self::Decode(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ValidationError> for TransportError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}

impl From<EncodeError> for TransportError {
    fn from(e: EncodeError) -> Self {
        Self::Encode(e)
    }
}

impl From<DecodeError> for TransportError {
    fn from(e: DecodeError) -> Self {
        Self::Decode(e)
    }
}

/// Descriptor validation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    SlotOutOfBounds {
        slot: u32,
        max: u32,
    },
    PayloadOutOfBounds {
        offset: u32,
        len: u32,
        slot_size: u32,
    },
    InlinePayloadTooLarge {
        len: u32,
        max: u32,
    },
    PayloadTooLarge {
        len: u32,
        max: u32,
    },
    StaleGeneration {
        expected: u32,
        actual: u32,
    },
    ChannelOutOfBounds {
        channel: u32,
        max: u32,
    },
    InvalidInlineDescriptor,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlotOutOfBounds { slot, max } => {
                write!(f, "slot {slot} out of bounds (max {max})")
            }
            Self::PayloadOutOfBounds {
                offset,
                len,
                slot_size,
            } => {
                write!(
                    f,
                    "payload [{offset}..{offset}+{len}] exceeds slot size {slot_size}"
                )
            }
            Self::InlinePayloadTooLarge { len, max } => {
                write!(f, "inline payload {len} bytes exceeds max {max}")
            }
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload {len} bytes exceeds max {max}")
            }
            Self::StaleGeneration { expected, actual } => {
                write!(f, "stale generation: expected {expected}, got {actual}")
            }
            Self::ChannelOutOfBounds { channel, max } => {
                write!(f, "channel {channel} out of bounds (max {max})")
            }
            Self::InvalidInlineDescriptor => {
                write!(f, "inline descriptor has non-zero slot fields")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Encoding errors.
#[derive(Debug)]
pub enum EncodeError {
    BufferTooSmall { needed: usize, available: usize },
    EncodeFailed(String),
    NoSlotAvailable,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { needed, available } => {
                write!(f, "buffer too small: need {needed}, have {available}")
            }
            Self::EncodeFailed(msg) => write!(f, "encode failed: {msg}"),
            Self::NoSlotAvailable => write!(f, "no slot available"),
        }
    }
}

impl std::error::Error for EncodeError {}

/// Decoding errors.
#[derive(Debug)]
pub enum DecodeError {
    UnexpectedEof,
    InvalidData(String),
    UnsupportedEncoding(u16),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::InvalidData(msg) => write!(f, "invalid data: {msg}"),
            Self::UnsupportedEncoding(enc) => write!(f, "unsupported encoding: {enc}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// High-level RPC errors.
#[derive(Debug)]
pub enum RpcError {
    Transport(TransportError),
    Status {
        code: ErrorCode,
        message: String,
    },
    Cancelled,
    DeadlineExceeded,
    /// Serialization error from facet-postcard
    Serialize(facet_postcard::SerializeError),
    /// Deserialization error from facet-postcard
    Deserialize(facet_postcard::DeserializeError),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::Status { code, message } => write!(f, "{code}: {message}"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::DeadlineExceeded => write!(f, "deadline exceeded"),
            Self::Serialize(e) => write!(f, "serialize error: {e}"),
            Self::Deserialize(e) => write!(f, "deserialize error: {e}"),
        }
    }
}

impl std::error::Error for RpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(e) => Some(e),
            Self::Serialize(e) => Some(e),
            Self::Deserialize(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TransportError> for RpcError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

impl From<facet_postcard::SerializeError> for RpcError {
    fn from(e: facet_postcard::SerializeError) -> Self {
        Self::Serialize(e)
    }
}

impl From<facet_postcard::DeserializeError> for RpcError {
    fn from(e: facet_postcard::DeserializeError) -> Self {
        Self::Deserialize(e)
    }
}
