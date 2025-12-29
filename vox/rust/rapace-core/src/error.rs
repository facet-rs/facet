//! Error codes and error types.
//!
//! See spec: [Error Handling](https://rapace.dev/spec/errors/)

use core::fmt;

/// RPC error codes.
///
/// Codes 0-49 align with gRPC for familiarity and interoperability.
/// Codes 50-99 are protocol/transport errors.
/// Codes 100+ are rapace-specific or application-defined.
///
/// Spec: `[impl error.status.success]` - code 0 means success.
/// Spec: `[impl error.status.error]` - non-zero code means error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, facet::Facet)]
#[repr(u32)]
pub enum ErrorCode {
    // gRPC-aligned (0-49)
    /// Success (not an error).
    Ok = 0,
    /// Request was canceled by client.
    Cancelled = 1,
    /// Deadline passed before completion.
    DeadlineExceeded = 2,
    /// Client sent invalid arguments.
    InvalidArgument = 3,
    /// Requested entity not found.
    NotFound = 4,
    /// Entity already exists.
    AlreadyExists = 5,
    /// Caller lacks permission.
    PermissionDenied = 6,
    /// Out of resources (memory, slots, quota).
    ResourceExhausted = 7,
    /// System not in required state.
    FailedPrecondition = 8,
    /// Operation aborted (e.g., concurrency conflict).
    Aborted = 9,
    /// Value out of valid range.
    OutOfRange = 10,
    /// Method not implemented.
    ///
    /// Spec: `[impl core.method-id.unknown-method]` - respond with UNIMPLEMENTED.
    Unimplemented = 11,
    /// Internal server error.
    Internal = 12,
    /// Service temporarily unavailable.
    Unavailable = 13,
    /// Unrecoverable data loss.
    DataLoss = 14,

    // rapace-specific (100+)
    /// Peer process died unexpectedly.
    PeerDied = 100,
    /// RPC session was closed.
    SessionClosed = 101,
    /// Frame or payload validation failed.
    ValidationFailed = 102,
    /// SHM slot generation mismatch (ABA detection).
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
#[derive(Debug, Clone, facet::Facet)]
#[repr(u8)]
pub enum TransportError {
    Closed,
    Io(IoError),
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
        Self::Io(e.into())
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
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
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

/// Serializable I/O error that captures the essential information from std::io::Error.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct IoError {
    pub kind: IoErrorKind,
    pub message: String,
}

impl IoError {
    pub fn new(kind: IoErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for IoError {}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        Self {
            kind: e.kind().into(),
            message: e.to_string(),
        }
    }
}

/// Serializable version of std::io::ErrorKind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum IoErrorKind {
    NotFound = 0,
    PermissionDenied = 1,
    ConnectionRefused = 2,
    ConnectionReset = 3,
    ConnectionAborted = 4,
    NotConnected = 5,
    AddrInUse = 6,
    AddrNotAvailable = 7,
    BrokenPipe = 8,
    AlreadyExists = 9,
    WouldBlock = 10,
    InvalidInput = 11,
    InvalidData = 12,
    TimedOut = 13,
    WriteZero = 14,
    Interrupted = 15,
    UnexpectedEof = 16,
    Other = 255,
}

impl fmt::Display for IoErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "entity not found"),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::ConnectionRefused => write!(f, "connection refused"),
            Self::ConnectionReset => write!(f, "connection reset"),
            Self::ConnectionAborted => write!(f, "connection aborted"),
            Self::NotConnected => write!(f, "not connected"),
            Self::AddrInUse => write!(f, "address in use"),
            Self::AddrNotAvailable => write!(f, "address not available"),
            Self::BrokenPipe => write!(f, "broken pipe"),
            Self::AlreadyExists => write!(f, "entity already exists"),
            Self::WouldBlock => write!(f, "operation would block"),
            Self::InvalidInput => write!(f, "invalid input parameter"),
            Self::InvalidData => write!(f, "invalid data"),
            Self::TimedOut => write!(f, "timed out"),
            Self::WriteZero => write!(f, "write zero"),
            Self::Interrupted => write!(f, "operation interrupted"),
            Self::UnexpectedEof => write!(f, "unexpected end of file"),
            Self::Other => write!(f, "other error"),
        }
    }
}

impl From<std::io::ErrorKind> for IoErrorKind {
    fn from(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::ConnectionRefused => Self::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset => Self::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted => Self::ConnectionAborted,
            std::io::ErrorKind::NotConnected => Self::NotConnected,
            std::io::ErrorKind::AddrInUse => Self::AddrInUse,
            std::io::ErrorKind::AddrNotAvailable => Self::AddrNotAvailable,
            std::io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::WouldBlock => Self::WouldBlock,
            std::io::ErrorKind::InvalidInput => Self::InvalidInput,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            std::io::ErrorKind::WriteZero => Self::WriteZero,
            std::io::ErrorKind::Interrupted => Self::Interrupted,
            std::io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            _ => Self::Other,
        }
    }
}

/// Encoding errors.
#[derive(Debug, Clone, facet::Facet)]
#[repr(u8)]
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

impl From<facet_format_postcard::SerializeError> for EncodeError {
    fn from(e: facet_format_postcard::SerializeError) -> Self {
        Self::EncodeFailed(e.to_string())
    }
}

/// Decoding errors.
#[derive(Debug, Clone, facet::Facet)]
#[repr(u8)]
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
#[derive(Debug, Clone, facet::Facet)]
#[repr(u8)]
pub enum RpcError {
    Transport(TransportError),
    Status {
        code: ErrorCode,
        message: String,
    },
    Cancelled,
    DeadlineExceeded,
    /// Serialization error - contains error message
    Serialize(String),
    /// Deserialization error - contains error message
    Deserialize(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::Status { code, message } => write!(f, "{code}: {message}"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::DeadlineExceeded => write!(f, "deadline exceeded"),
            Self::Serialize(msg) => write!(f, "could not serialize to postcard: {msg}"),
            Self::Deserialize(msg) => write!(f, "could not deserialize from postcard: {msg}"),
        }
    }
}

impl std::error::Error for RpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TransportError> for RpcError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

impl From<facet_format_postcard::SerializeError> for RpcError {
    fn from(e: facet_format_postcard::SerializeError) -> Self {
        Self::Serialize(e.to_string())
    }
}

impl From<facet_format_postcard::DeserializeError<facet_format_postcard::PostcardError>>
    for RpcError
{
    fn from(
        e: facet_format_postcard::DeserializeError<facet_format_postcard::PostcardError>,
    ) -> Self {
        Self::Deserialize(e.to_string())
    }
}

impl From<EncodeError> for RpcError {
    fn from(e: EncodeError) -> Self {
        Self::Serialize(e.to_string())
    }
}

impl From<DecodeError> for RpcError {
    fn from(e: DecodeError) -> Self {
        Self::Deserialize(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_error_implements_facet() {
        // This test verifies that RpcError implements Facet
        // by checking that we can get its shape
        let _ = facet::shape_of::<RpcError>();
    }

    #[test]
    fn test_transport_error_implements_facet() {
        let _ = facet::shape_of::<TransportError>();
    }

    #[test]
    fn test_io_error_roundtrip() {
        let original = std::io::Error::new(std::io::ErrorKind::NotFound, "test file not found");
        let io_error: IoError = original.into();
        assert_eq!(io_error.kind, IoErrorKind::NotFound);
        assert!(io_error.message.contains("test file not found"));
    }
}
