//! Error types for the HTTP bridge.

use std::fmt;

/// Protocol-level errors from roam.
///
/// r[bridge.response.protocol-error]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    /// Method not found on the service.
    UnknownMethod,
    /// Request payload could not be decoded.
    InvalidPayload,
    /// Request was cancelled.
    Cancelled,
}

impl ProtocolErrorKind {
    /// Convert to JSON bytes for HTTP response.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        match self {
            ProtocolErrorKind::UnknownMethod => br#"{"error":"unknown_method"}"#.to_vec(),
            ProtocolErrorKind::InvalidPayload => br#"{"error":"invalid_payload"}"#.to_vec(),
            ProtocolErrorKind::Cancelled => br#"{"error":"cancelled"}"#.to_vec(),
        }
    }
}

/// Bridge-level errors (transport failures, etc.).
///
/// r[bridge.response.bridge-error]
#[derive(Debug)]
pub struct BridgeError {
    /// HTTP status code to return.
    pub status: http::StatusCode,
    /// Human-readable error message.
    pub message: String,
}

impl BridgeError {
    /// Create a new bridge error.
    pub fn new(status: http::StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Backend service is unavailable (502 Bad Gateway).
    pub fn backend_unavailable(message: impl Into<String>) -> Self {
        Self::new(http::StatusCode::BAD_GATEWAY, message)
    }

    /// Request timed out (504 Gateway Timeout).
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(http::StatusCode::GATEWAY_TIMEOUT, message)
    }

    /// Bad request (400).
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(http::StatusCode::BAD_REQUEST, message)
    }

    /// Internal error (500).
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(http::StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    /// Convert to JSON bytes for HTTP response body.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        // r[bridge.response.bridge-error]
        // Simple JSON escaping for the message
        let escaped = self
            .message
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!(r#"{{"error":"bridge","message":"{}"}}"#, escaped).into_bytes()
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.status, self.message)
    }
}

impl std::error::Error for BridgeError {}
