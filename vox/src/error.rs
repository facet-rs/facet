// src/error.rs

use std::fmt;

/// Error codes aligned with gRPC (0-99) plus rapace-specific (100+).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // ===== gRPC-aligned codes (0-99) =====

    /// Success (not an error)
    Ok = 0,

    // Cancellation & timeouts
    /// Operation was cancelled
    Cancelled = 1,
    /// Deadline passed before completion
    DeadlineExceeded = 2,

    // Request errors
    /// Malformed request
    InvalidArgument = 3,
    /// Service/method not found
    NotFound = 4,
    /// Resource already exists
    AlreadyExists = 5,
    /// Caller lacks permission
    PermissionDenied = 6,

    // Resource errors
    /// Out of credits, slots, channels, etc.
    ResourceExhausted = 7,
    /// System not in required state
    FailedPrecondition = 8,

    // Protocol errors
    /// Operation aborted (conflict, etc.)
    Aborted = 9,
    /// Value out of valid range
    OutOfRange = 10,
    /// Method not implemented
    Unimplemented = 11,

    // System errors
    /// Internal error (bug)
    Internal = 12,
    /// Service temporarily unavailable
    Unavailable = 13,
    /// Unrecoverable data loss
    DataLoss = 14,

    // ===== rapace-specific codes (100+) =====

    /// Peer process crashed
    PeerDied = 100,
    /// Session shut down
    SessionClosed = 101,
    /// Descriptor validation failed
    ValidationFailed = 102,
    /// Generation counter mismatch
    StaleGeneration = 103,
}

impl ErrorCode {
    /// Convert from a u32 wire value.
    /// Returns None if the value doesn't match a known error code.
    pub fn from_u32(val: u32) -> Option<Self> {
        Some(match val {
            0 => ErrorCode::Ok,
            1 => ErrorCode::Cancelled,
            2 => ErrorCode::DeadlineExceeded,
            3 => ErrorCode::InvalidArgument,
            4 => ErrorCode::NotFound,
            5 => ErrorCode::AlreadyExists,
            6 => ErrorCode::PermissionDenied,
            7 => ErrorCode::ResourceExhausted,
            8 => ErrorCode::FailedPrecondition,
            9 => ErrorCode::Aborted,
            10 => ErrorCode::OutOfRange,
            11 => ErrorCode::Unimplemented,
            12 => ErrorCode::Internal,
            13 => ErrorCode::Unavailable,
            14 => ErrorCode::DataLoss,
            100 => ErrorCode::PeerDied,
            101 => ErrorCode::SessionClosed,
            102 => ErrorCode::ValidationFailed,
            103 => ErrorCode::StaleGeneration,
            _ => return None,
        })
    }

    /// Convert to u32 for wire transmission.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Check if this error is retryable.
    ///
    /// Retryable errors are temporary conditions that may succeed on retry:
    /// - Cancelled (operation may be retried with new context)
    /// - ResourceExhausted (resources may become available)
    /// - Unavailable (service may recover)
    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            ErrorCode::Cancelled
                | ErrorCode::ResourceExhausted
                | ErrorCode::Unavailable
        )
    }

    /// Check if this error is fatal.
    ///
    /// Fatal errors indicate unrecoverable conditions:
    /// - PeerDied (peer process crashed)
    /// - SessionClosed (session shut down)
    /// - DataLoss (unrecoverable data corruption)
    pub fn is_fatal(self) -> bool {
        matches!(
            self,
            ErrorCode::PeerDied
                | ErrorCode::SessionClosed
                | ErrorCode::DataLoss
        )
    }

    /// Check if this error indicates a client-side problem.
    ///
    /// Client errors are caused by invalid requests or client state:
    /// - InvalidArgument (malformed request)
    /// - NotFound (resource doesn't exist)
    /// - AlreadyExists (resource already present)
    /// - PermissionDenied (insufficient permissions)
    /// - OutOfRange (value out of bounds)
    pub fn is_client_error(self) -> bool {
        matches!(
            self,
            ErrorCode::InvalidArgument
                | ErrorCode::NotFound
                | ErrorCode::AlreadyExists
                | ErrorCode::PermissionDenied
                | ErrorCode::OutOfRange
        )
    }

    /// Check if this error indicates a server-side problem.
    ///
    /// Server errors are internal to the service implementation:
    /// - Unimplemented (method not implemented)
    /// - Internal (server bug)
    /// - DataLoss (server data corruption)
    pub fn is_server_error(self) -> bool {
        matches!(
            self,
            ErrorCode::Unimplemented
                | ErrorCode::Internal
                | ErrorCode::DataLoss
        )
    }

    /// Get a human-readable description of this error code.
    pub fn description(self) -> &'static str {
        match self {
            ErrorCode::Ok => "success",
            ErrorCode::Cancelled => "operation was cancelled",
            ErrorCode::DeadlineExceeded => "deadline exceeded",
            ErrorCode::InvalidArgument => "invalid argument",
            ErrorCode::NotFound => "not found",
            ErrorCode::AlreadyExists => "already exists",
            ErrorCode::PermissionDenied => "permission denied",
            ErrorCode::ResourceExhausted => "resource exhausted",
            ErrorCode::FailedPrecondition => "failed precondition",
            ErrorCode::Aborted => "operation aborted",
            ErrorCode::OutOfRange => "out of range",
            ErrorCode::Unimplemented => "not implemented",
            ErrorCode::Internal => "internal error",
            ErrorCode::Unavailable => "service unavailable",
            ErrorCode::DataLoss => "data loss",
            ErrorCode::PeerDied => "peer died",
            ErrorCode::SessionClosed => "session closed",
            ErrorCode::ValidationFailed => "validation failed",
            ErrorCode::StaleGeneration => "stale generation",
        }
    }
}

impl TryFrom<u32> for ErrorCode {
    type Error = UnknownErrorCode;

    fn try_from(val: u32) -> std::result::Result<Self, Self::Error> {
        ErrorCode::from_u32(val).ok_or(UnknownErrorCode(val))
    }
}

impl From<ErrorCode> for u32 {
    fn from(code: ErrorCode) -> u32 {
        code.as_u32()
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.description(), self.as_u32())
    }
}

/// Error when converting from an unknown u32 error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownErrorCode(pub u32);

impl fmt::Display for UnknownErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown error code: {}", self.0)
    }
}

impl std::error::Error for UnknownErrorCode {}

/// A rapace error with code, message, and optional source.
#[derive(Debug)]
pub struct RapaceError {
    code: ErrorCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl RapaceError {
    /// Create a new error with the given code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        RapaceError {
            code,
            message: message.into(),
            source: None,
        }
    }

    /// Create a new error with the given code, message, and source error.
    pub fn with_source(
        code: ErrorCode,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        RapaceError {
            code,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Get the error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Get the error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Check if this error is retryable.
    pub fn is_retryable(&self) -> bool {
        self.code.is_retryable()
    }

    /// Check if this error is fatal.
    pub fn is_fatal(&self) -> bool {
        self.code.is_fatal()
    }
}

impl fmt::Display for RapaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RapaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}

// Convenience constructors for common error types

impl RapaceError {
    /// Create a Cancelled error.
    pub fn cancelled(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::Cancelled, message)
    }

    /// Create a DeadlineExceeded error.
    pub fn deadline_exceeded(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::DeadlineExceeded, message)
    }

    /// Create an InvalidArgument error.
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::InvalidArgument, message)
    }

    /// Create a NotFound error.
    pub fn not_found(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::NotFound, message)
    }

    /// Create a ResourceExhausted error.
    pub fn resource_exhausted(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::ResourceExhausted, message)
    }

    /// Create an Internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::Internal, message)
    }

    /// Create a PeerDied error.
    pub fn peer_died(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::PeerDied, message)
    }

    /// Create a SessionClosed error.
    pub fn session_closed(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::SessionClosed, message)
    }

    /// Create a ValidationFailed error.
    pub fn validation_failed(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::ValidationFailed, message)
    }

    /// Create a StaleGeneration error.
    pub fn stale_generation(message: impl Into<String>) -> Self {
        RapaceError::new(ErrorCode::StaleGeneration, message)
    }
}

/// Result type alias for rapace operations.
pub type Result<T> = std::result::Result<T, RapaceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_roundtrip() {
        let codes = [
            ErrorCode::Ok,
            ErrorCode::Cancelled,
            ErrorCode::DeadlineExceeded,
            ErrorCode::InvalidArgument,
            ErrorCode::NotFound,
            ErrorCode::AlreadyExists,
            ErrorCode::PermissionDenied,
            ErrorCode::ResourceExhausted,
            ErrorCode::FailedPrecondition,
            ErrorCode::Aborted,
            ErrorCode::OutOfRange,
            ErrorCode::Unimplemented,
            ErrorCode::Internal,
            ErrorCode::Unavailable,
            ErrorCode::DataLoss,
            ErrorCode::PeerDied,
            ErrorCode::SessionClosed,
            ErrorCode::ValidationFailed,
            ErrorCode::StaleGeneration,
        ];

        for &code in &codes {
            let val = code.as_u32();
            let roundtrip = ErrorCode::from_u32(val).unwrap();
            assert_eq!(code, roundtrip);
        }
    }

    #[test]
    fn error_code_try_from() {
        assert_eq!(ErrorCode::try_from(0).unwrap(), ErrorCode::Ok);
        assert_eq!(ErrorCode::try_from(1).unwrap(), ErrorCode::Cancelled);
        assert_eq!(ErrorCode::try_from(100).unwrap(), ErrorCode::PeerDied);

        assert_eq!(ErrorCode::try_from(999), Err(UnknownErrorCode(999)));
    }

    #[test]
    fn error_code_is_retryable() {
        assert!(ErrorCode::Cancelled.is_retryable());
        assert!(ErrorCode::ResourceExhausted.is_retryable());
        assert!(ErrorCode::Unavailable.is_retryable());

        assert!(!ErrorCode::InvalidArgument.is_retryable());
        assert!(!ErrorCode::NotFound.is_retryable());
        assert!(!ErrorCode::PeerDied.is_retryable());
    }

    #[test]
    fn error_code_is_fatal() {
        assert!(ErrorCode::PeerDied.is_fatal());
        assert!(ErrorCode::SessionClosed.is_fatal());
        assert!(ErrorCode::DataLoss.is_fatal());

        assert!(!ErrorCode::Cancelled.is_fatal());
        assert!(!ErrorCode::InvalidArgument.is_fatal());
        assert!(!ErrorCode::Unavailable.is_fatal());
    }

    #[test]
    fn error_code_is_client_error() {
        assert!(ErrorCode::InvalidArgument.is_client_error());
        assert!(ErrorCode::NotFound.is_client_error());
        assert!(ErrorCode::AlreadyExists.is_client_error());
        assert!(ErrorCode::PermissionDenied.is_client_error());
        assert!(ErrorCode::OutOfRange.is_client_error());

        assert!(!ErrorCode::Internal.is_client_error());
        assert!(!ErrorCode::Unavailable.is_client_error());
    }

    #[test]
    fn error_code_is_server_error() {
        assert!(ErrorCode::Unimplemented.is_server_error());
        assert!(ErrorCode::Internal.is_server_error());
        assert!(ErrorCode::DataLoss.is_server_error());

        assert!(!ErrorCode::InvalidArgument.is_server_error());
        assert!(!ErrorCode::NotFound.is_server_error());
    }

    #[test]
    fn error_code_description() {
        assert_eq!(ErrorCode::Cancelled.description(), "operation was cancelled");
        assert_eq!(ErrorCode::NotFound.description(), "not found");
        assert_eq!(ErrorCode::PeerDied.description(), "peer died");
    }

    #[test]
    fn error_code_display() {
        let s = format!("{}", ErrorCode::Cancelled);
        assert!(s.contains("cancelled"));
        assert!(s.contains("1"));
    }

    #[test]
    fn rapace_error_creation() {
        let err = RapaceError::new(ErrorCode::NotFound, "service not found");
        assert_eq!(err.code(), ErrorCode::NotFound);
        assert_eq!(err.message(), "service not found");
        assert!(!err.is_retryable());
        assert!(!err.is_fatal());
    }

    #[test]
    fn rapace_error_with_source() {
        use std::error::Error;

        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = RapaceError::with_source(
            ErrorCode::Internal,
            "failed to load config",
            source,
        );

        assert_eq!(err.code(), ErrorCode::Internal);
        assert_eq!(err.message(), "failed to load config");
        assert!(err.source().is_some());
    }

    #[test]
    fn rapace_error_display() {
        let err = RapaceError::cancelled("user cancelled operation");
        let s = format!("{}", err);
        assert!(s.contains("cancelled"));
        assert!(s.contains("user cancelled operation"));
    }

    #[test]
    fn rapace_error_convenience_constructors() {
        assert_eq!(RapaceError::cancelled("x").code(), ErrorCode::Cancelled);
        assert_eq!(RapaceError::deadline_exceeded("x").code(), ErrorCode::DeadlineExceeded);
        assert_eq!(RapaceError::invalid_argument("x").code(), ErrorCode::InvalidArgument);
        assert_eq!(RapaceError::not_found("x").code(), ErrorCode::NotFound);
        assert_eq!(RapaceError::resource_exhausted("x").code(), ErrorCode::ResourceExhausted);
        assert_eq!(RapaceError::internal("x").code(), ErrorCode::Internal);
        assert_eq!(RapaceError::peer_died("x").code(), ErrorCode::PeerDied);
        assert_eq!(RapaceError::session_closed("x").code(), ErrorCode::SessionClosed);
        assert_eq!(RapaceError::validation_failed("x").code(), ErrorCode::ValidationFailed);
        assert_eq!(RapaceError::stale_generation("x").code(), ErrorCode::StaleGeneration);
    }

    #[test]
    fn unknown_error_code_display() {
        let err = UnknownErrorCode(999);
        let s = format!("{}", err);
        assert!(s.contains("999"));
    }

    #[test]
    fn error_code_values_match_spec() {
        // Verify gRPC-aligned codes
        assert_eq!(ErrorCode::Ok as u32, 0);
        assert_eq!(ErrorCode::Cancelled as u32, 1);
        assert_eq!(ErrorCode::DeadlineExceeded as u32, 2);
        assert_eq!(ErrorCode::InvalidArgument as u32, 3);
        assert_eq!(ErrorCode::NotFound as u32, 4);
        assert_eq!(ErrorCode::AlreadyExists as u32, 5);
        assert_eq!(ErrorCode::PermissionDenied as u32, 6);
        assert_eq!(ErrorCode::ResourceExhausted as u32, 7);
        assert_eq!(ErrorCode::FailedPrecondition as u32, 8);
        assert_eq!(ErrorCode::Aborted as u32, 9);
        assert_eq!(ErrorCode::OutOfRange as u32, 10);
        assert_eq!(ErrorCode::Unimplemented as u32, 11);
        assert_eq!(ErrorCode::Internal as u32, 12);
        assert_eq!(ErrorCode::Unavailable as u32, 13);
        assert_eq!(ErrorCode::DataLoss as u32, 14);

        // Verify rapace-specific codes
        assert_eq!(ErrorCode::PeerDied as u32, 100);
        assert_eq!(ErrorCode::SessionClosed as u32, 101);
        assert_eq!(ErrorCode::ValidationFailed as u32, 102);
        assert_eq!(ErrorCode::StaleGeneration as u32, 103);
    }
}
