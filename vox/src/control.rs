// src/control.rs

use crate::codec::{Codec, PostcardCodec};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum service name length in control messages.
pub const MAX_SERVICE_NAME_LEN: usize = 256;

/// Maximum method name length in control messages.
pub const MAX_METHOD_NAME_LEN: usize = 128;

/// Maximum metadata key length.
pub const MAX_METADATA_KEY_LEN: usize = 64;

/// Maximum metadata value length.
pub const MAX_METADATA_VALUE_LEN: usize = 4096;

/// Maximum number of metadata pairs in a single message.
pub const MAX_METADATA_PAIRS: usize = 32;

/// Control payload for channel 0 control messages.
///
/// Each variant corresponds to a specific method_id from the ControlMethod enum.
/// Control payloads are serialized using postcard encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlPayload {
    /// method_id = 1: Open a new data channel
    OpenChannel {
        channel_id: u32,
        service_name: String,
        method_name: String,
        metadata: Vec<(String, Vec<u8>)>,
    },

    /// method_id = 2: Close a channel gracefully
    CloseChannel {
        channel_id: u32,
        reason: CloseReason,
    },

    /// method_id = 3: Cancel a channel (advisory)
    CancelChannel {
        channel_id: u32,
        reason: CancelReason,
    },

    /// method_id = 4: Grant flow control credits
    GrantCredits { channel_id: u32, bytes: u32 },

    /// method_id = 5: Liveness probe
    Ping { payload: [u8; 8] },

    /// method_id = 6: Response to Ping
    Pong { payload: [u8; 8] },
}

/// Reason for gracefully closing a channel.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CloseReason {
    /// Normal closure (both sides agreed to close)
    Normal = 0,
    /// One side is going away (shutdown, restart)
    GoingAway = 1,
    /// Protocol error detected
    ProtocolError = 2,
}

impl CloseReason {
    /// Convert from a u8 wire value.
    pub fn from_u8(val: u8) -> Option<Self> {
        Some(match val {
            0 => CloseReason::Normal,
            1 => CloseReason::GoingAway,
            2 => CloseReason::ProtocolError,
            _ => return None,
        })
    }

    /// Convert to u8 for wire transmission.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Get a human-readable description of this close reason.
    pub fn description(self) -> &'static str {
        match self {
            CloseReason::Normal => "normal closure",
            CloseReason::GoingAway => "going away",
            CloseReason::ProtocolError => "protocol error",
        }
    }
}

impl fmt::Display for CloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Reason for cancelling a channel.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CancelReason {
    /// User requested cancellation
    UserRequested = 0,
    /// Operation timed out
    Timeout = 1,
    /// Deadline exceeded
    DeadlineExceeded = 2,
    /// Resource exhausted
    ResourceExhausted = 3,
    /// Peer process died
    PeerDied = 4,
}

impl CancelReason {
    /// Convert from a u8 wire value.
    pub fn from_u8(val: u8) -> Option<Self> {
        Some(match val {
            0 => CancelReason::UserRequested,
            1 => CancelReason::Timeout,
            2 => CancelReason::DeadlineExceeded,
            3 => CancelReason::ResourceExhausted,
            4 => CancelReason::PeerDied,
            _ => return None,
        })
    }

    /// Convert to u8 for wire transmission.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Get a human-readable description of this cancel reason.
    pub fn description(self) -> &'static str {
        match self {
            CancelReason::UserRequested => "user requested",
            CancelReason::Timeout => "timeout",
            CancelReason::DeadlineExceeded => "deadline exceeded",
            CancelReason::ResourceExhausted => "resource exhausted",
            CancelReason::PeerDied => "peer died",
        }
    }
}

impl fmt::Display for CancelReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Error type for control payload operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    /// Service name exceeds maximum length
    ServiceNameTooLong { len: usize, max: usize },
    /// Method name exceeds maximum length
    MethodNameTooLong { len: usize, max: usize },
    /// Metadata key exceeds maximum length
    MetadataKeyTooLong { len: usize, max: usize },
    /// Metadata value exceeds maximum length
    MetadataValueTooLong { len: usize, max: usize },
    /// Too many metadata pairs
    TooManyMetadataPairs { count: usize, max: usize },
    /// Encoding error
    EncodingError(String),
    /// Decoding error
    DecodingError(String),
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlError::ServiceNameTooLong { len, max } => {
                write!(
                    f,
                    "service name too long: {} bytes (max {})",
                    len, max
                )
            }
            ControlError::MethodNameTooLong { len, max } => {
                write!(f, "method name too long: {} bytes (max {})", len, max)
            }
            ControlError::MetadataKeyTooLong { len, max } => {
                write!(f, "metadata key too long: {} bytes (max {})", len, max)
            }
            ControlError::MetadataValueTooLong { len, max } => {
                write!(
                    f,
                    "metadata value too long: {} bytes (max {})",
                    len, max
                )
            }
            ControlError::TooManyMetadataPairs { count, max } => {
                write!(f, "too many metadata pairs: {} (max {})", count, max)
            }
            ControlError::EncodingError(msg) => write!(f, "encoding error: {}", msg),
            ControlError::DecodingError(msg) => write!(f, "decoding error: {}", msg),
        }
    }
}

impl std::error::Error for ControlError {}

impl ControlPayload {
    /// Encode this control payload to bytes using postcard.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        // Validate before encoding
        self.validate()?;

        PostcardCodec::encode(self)
            .map_err(|e| ControlError::EncodingError(e.to_string()))
    }

    /// Decode a control payload from bytes using postcard.
    pub fn decode(buf: &[u8]) -> Result<Self, ControlError> {
        let payload: Self = PostcardCodec::decode(buf)
            .map_err(|e| ControlError::DecodingError(e.to_string()))?;

        // Validate after decoding
        payload.validate()?;

        Ok(payload)
    }

    /// Validate this control payload against length limits.
    pub fn validate(&self) -> Result<(), ControlError> {
        match self {
            ControlPayload::OpenChannel {
                service_name,
                method_name,
                metadata,
                ..
            } => {
                // Validate service name length
                if service_name.len() > MAX_SERVICE_NAME_LEN {
                    return Err(ControlError::ServiceNameTooLong {
                        len: service_name.len(),
                        max: MAX_SERVICE_NAME_LEN,
                    });
                }

                // Validate method name length
                if method_name.len() > MAX_METHOD_NAME_LEN {
                    return Err(ControlError::MethodNameTooLong {
                        len: method_name.len(),
                        max: MAX_METHOD_NAME_LEN,
                    });
                }

                // Validate metadata pairs count
                if metadata.len() > MAX_METADATA_PAIRS {
                    return Err(ControlError::TooManyMetadataPairs {
                        count: metadata.len(),
                        max: MAX_METADATA_PAIRS,
                    });
                }

                // Validate each metadata key/value
                for (key, value) in metadata {
                    if key.len() > MAX_METADATA_KEY_LEN {
                        return Err(ControlError::MetadataKeyTooLong {
                            len: key.len(),
                            max: MAX_METADATA_KEY_LEN,
                        });
                    }
                    if value.len() > MAX_METADATA_VALUE_LEN {
                        return Err(ControlError::MetadataValueTooLong {
                            len: value.len(),
                            max: MAX_METADATA_VALUE_LEN,
                        });
                    }
                }

                Ok(())
            }
            // Other variants have no validation requirements
            _ => Ok(()),
        }
    }

    /// Create an OpenChannel payload.
    pub fn open_channel(
        channel_id: u32,
        service_name: impl Into<String>,
        method_name: impl Into<String>,
    ) -> Self {
        ControlPayload::OpenChannel {
            channel_id,
            service_name: service_name.into(),
            method_name: method_name.into(),
            metadata: Vec::new(),
        }
    }

    /// Create an OpenChannel payload with metadata.
    pub fn open_channel_with_metadata(
        channel_id: u32,
        service_name: impl Into<String>,
        method_name: impl Into<String>,
        metadata: Vec<(String, Vec<u8>)>,
    ) -> Self {
        ControlPayload::OpenChannel {
            channel_id,
            service_name: service_name.into(),
            method_name: method_name.into(),
            metadata,
        }
    }

    /// Create a CloseChannel payload.
    pub fn close_channel(channel_id: u32, reason: CloseReason) -> Self {
        ControlPayload::CloseChannel { channel_id, reason }
    }

    /// Create a CancelChannel payload.
    pub fn cancel_channel(channel_id: u32, reason: CancelReason) -> Self {
        ControlPayload::CancelChannel { channel_id, reason }
    }

    /// Create a GrantCredits payload.
    pub fn grant_credits(channel_id: u32, bytes: u32) -> Self {
        ControlPayload::GrantCredits { channel_id, bytes }
    }

    /// Create a Ping payload with the given payload.
    pub fn ping(payload: [u8; 8]) -> Self {
        ControlPayload::Ping { payload }
    }

    /// Create a Pong payload with the given payload.
    pub fn pong(payload: [u8; 8]) -> Self {
        ControlPayload::Pong { payload }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_reason_roundtrip() {
        let reasons = [
            CloseReason::Normal,
            CloseReason::GoingAway,
            CloseReason::ProtocolError,
        ];

        for &reason in &reasons {
            let val = reason.as_u8();
            let roundtrip = CloseReason::from_u8(val).unwrap();
            assert_eq!(reason, roundtrip);
        }
    }

    #[test]
    fn close_reason_values_match_spec() {
        assert_eq!(CloseReason::Normal as u8, 0);
        assert_eq!(CloseReason::GoingAway as u8, 1);
        assert_eq!(CloseReason::ProtocolError as u8, 2);
    }

    #[test]
    fn close_reason_display() {
        assert_eq!(format!("{}", CloseReason::Normal), "normal closure");
        assert_eq!(format!("{}", CloseReason::GoingAway), "going away");
        assert_eq!(
            format!("{}", CloseReason::ProtocolError),
            "protocol error"
        );
    }

    #[test]
    fn cancel_reason_roundtrip() {
        let reasons = [
            CancelReason::UserRequested,
            CancelReason::Timeout,
            CancelReason::DeadlineExceeded,
            CancelReason::ResourceExhausted,
            CancelReason::PeerDied,
        ];

        for &reason in &reasons {
            let val = reason.as_u8();
            let roundtrip = CancelReason::from_u8(val).unwrap();
            assert_eq!(reason, roundtrip);
        }
    }

    #[test]
    fn cancel_reason_values_match_spec() {
        assert_eq!(CancelReason::UserRequested as u8, 0);
        assert_eq!(CancelReason::Timeout as u8, 1);
        assert_eq!(CancelReason::DeadlineExceeded as u8, 2);
        assert_eq!(CancelReason::ResourceExhausted as u8, 3);
        assert_eq!(CancelReason::PeerDied as u8, 4);
    }

    #[test]
    fn cancel_reason_display() {
        assert_eq!(format!("{}", CancelReason::UserRequested), "user requested");
        assert_eq!(format!("{}", CancelReason::Timeout), "timeout");
        assert_eq!(
            format!("{}", CancelReason::DeadlineExceeded),
            "deadline exceeded"
        );
        assert_eq!(
            format!("{}", CancelReason::ResourceExhausted),
            "resource exhausted"
        );
        assert_eq!(format!("{}", CancelReason::PeerDied), "peer died");
    }

    #[test]
    fn open_channel_constructor() {
        let payload = ControlPayload::open_channel(42, "service", "method");
        match payload {
            ControlPayload::OpenChannel {
                channel_id,
                service_name,
                method_name,
                metadata,
            } => {
                assert_eq!(channel_id, 42);
                assert_eq!(service_name, "service");
                assert_eq!(method_name, "method");
                assert!(metadata.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn open_channel_with_metadata_constructor() {
        let meta = vec![("key".to_string(), vec![1, 2, 3])];
        let payload =
            ControlPayload::open_channel_with_metadata(42, "service", "method", meta.clone());

        match payload {
            ControlPayload::OpenChannel {
                channel_id,
                service_name,
                method_name,
                metadata,
            } => {
                assert_eq!(channel_id, 42);
                assert_eq!(service_name, "service");
                assert_eq!(method_name, "method");
                assert_eq!(metadata, meta);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn close_channel_constructor() {
        let payload = ControlPayload::close_channel(42, CloseReason::Normal);
        match payload {
            ControlPayload::CloseChannel { channel_id, reason } => {
                assert_eq!(channel_id, 42);
                assert_eq!(reason, CloseReason::Normal);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn cancel_channel_constructor() {
        let payload = ControlPayload::cancel_channel(42, CancelReason::Timeout);
        match payload {
            ControlPayload::CancelChannel { channel_id, reason } => {
                assert_eq!(channel_id, 42);
                assert_eq!(reason, CancelReason::Timeout);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn grant_credits_constructor() {
        let payload = ControlPayload::grant_credits(42, 1024);
        match payload {
            ControlPayload::GrantCredits { channel_id, bytes } => {
                assert_eq!(channel_id, 42);
                assert_eq!(bytes, 1024);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn ping_constructor() {
        let payload = ControlPayload::ping([1, 2, 3, 4, 5, 6, 7, 8]);
        match payload {
            ControlPayload::Ping { payload: p } => {
                assert_eq!(p, [1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn pong_constructor() {
        let payload = ControlPayload::pong([1, 2, 3, 4, 5, 6, 7, 8]);
        match payload {
            ControlPayload::Pong { payload: p } => {
                assert_eq!(p, [1, 2, 3, 4, 5, 6, 7, 8]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn encode_decode_roundtrip_open_channel() {
        let original = ControlPayload::open_channel(42, "service", "method");
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_roundtrip_close_channel() {
        let original = ControlPayload::close_channel(42, CloseReason::Normal);
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_roundtrip_cancel_channel() {
        let original = ControlPayload::cancel_channel(42, CancelReason::Timeout);
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_roundtrip_grant_credits() {
        let original = ControlPayload::grant_credits(42, 1024);
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_roundtrip_ping() {
        let original = ControlPayload::ping([1, 2, 3, 4, 5, 6, 7, 8]);
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_roundtrip_pong() {
        let original = ControlPayload::pong([8, 7, 6, 5, 4, 3, 2, 1]);
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_roundtrip_with_metadata() {
        let metadata = vec![
            ("key1".to_string(), vec![1, 2, 3]),
            ("key2".to_string(), vec![4, 5, 6]),
        ];
        let original =
            ControlPayload::open_channel_with_metadata(42, "service", "method", metadata);
        let encoded = original.encode().unwrap();
        let decoded = ControlPayload::decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn validate_service_name_too_long() {
        let long_name = "a".repeat(MAX_SERVICE_NAME_LEN + 1);
        let payload = ControlPayload::open_channel(42, long_name, "method");
        let result = payload.validate();
        assert!(matches!(
            result,
            Err(ControlError::ServiceNameTooLong { .. })
        ));
    }

    #[test]
    fn validate_method_name_too_long() {
        let long_name = "a".repeat(MAX_METHOD_NAME_LEN + 1);
        let payload = ControlPayload::open_channel(42, "service", long_name);
        let result = payload.validate();
        assert!(matches!(
            result,
            Err(ControlError::MethodNameTooLong { .. })
        ));
    }

    #[test]
    fn validate_metadata_key_too_long() {
        let long_key = "a".repeat(MAX_METADATA_KEY_LEN + 1);
        let metadata = vec![(long_key, vec![1, 2, 3])];
        let payload =
            ControlPayload::open_channel_with_metadata(42, "service", "method", metadata);
        let result = payload.validate();
        assert!(matches!(
            result,
            Err(ControlError::MetadataKeyTooLong { .. })
        ));
    }

    #[test]
    fn validate_metadata_value_too_long() {
        let long_value = vec![0u8; MAX_METADATA_VALUE_LEN + 1];
        let metadata = vec![("key".to_string(), long_value)];
        let payload =
            ControlPayload::open_channel_with_metadata(42, "service", "method", metadata);
        let result = payload.validate();
        assert!(matches!(
            result,
            Err(ControlError::MetadataValueTooLong { .. })
        ));
    }

    #[test]
    fn validate_too_many_metadata_pairs() {
        let metadata = (0..MAX_METADATA_PAIRS + 1)
            .map(|i| (format!("key{}", i), vec![1, 2, 3]))
            .collect();
        let payload =
            ControlPayload::open_channel_with_metadata(42, "service", "method", metadata);
        let result = payload.validate();
        assert!(matches!(
            result,
            Err(ControlError::TooManyMetadataPairs { .. })
        ));
    }

    #[test]
    fn validate_at_max_limits_succeeds() {
        let service_name = "a".repeat(MAX_SERVICE_NAME_LEN);
        let method_name = "b".repeat(MAX_METHOD_NAME_LEN);
        let metadata = (0..MAX_METADATA_PAIRS)
            .map(|i| {
                let key = format!("k{}", i);
                let value = vec![0u8; MAX_METADATA_VALUE_LEN];
                (key, value)
            })
            .collect();

        let payload =
            ControlPayload::open_channel_with_metadata(42, service_name, method_name, metadata);
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn encode_rejects_invalid_payload() {
        let long_name = "a".repeat(MAX_SERVICE_NAME_LEN + 1);
        let payload = ControlPayload::open_channel(42, long_name, "method");
        let result = payload.encode();
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_invalid_payload() {
        // Create an invalid payload by bypassing validation
        let invalid = ControlPayload::OpenChannel {
            channel_id: 42,
            service_name: "a".repeat(MAX_SERVICE_NAME_LEN + 1),
            method_name: "method".to_string(),
            metadata: Vec::new(),
        };

        // Encode without validation (using postcard directly)
        let encoded = PostcardCodec::encode(&invalid).unwrap();

        // Decode should fail validation
        let result = ControlPayload::decode(&encoded);
        assert!(result.is_err());
    }

    #[test]
    fn control_error_display() {
        let err = ControlError::ServiceNameTooLong {
            len: 300,
            max: 256,
        };
        let s = format!("{}", err);
        assert!(s.contains("300"));
        assert!(s.contains("256"));

        let err = ControlError::EncodingError("test error".to_string());
        let s = format!("{}", err);
        assert!(s.contains("test error"));
    }
}
