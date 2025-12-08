// src/codec.rs

use serde::{de::DeserializeOwned, Serialize};
use std::fmt;

/// Encoding format identifier for message serialization.
///
/// This enum is wire-compatible and uses u16 representation for compact encoding.
/// The encoding type is transmitted in message headers to allow peers to decode
/// messages correctly.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// Postcard: compact binary format (default for control messages).
    Postcard = 1,
    /// JSON: human-readable format (for debugging/tooling).
    Json = 2,
    /// Raw: no serialization, passes bytes through as-is.
    Raw = 3,
}

impl TryFrom<u16> for Encoding {
    type Error = UnknownEncoding;

    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(Encoding::Postcard),
            2 => Ok(Encoding::Json),
            3 => Ok(Encoding::Raw),
            _ => Err(UnknownEncoding(v)),
        }
    }
}

impl From<Encoding> for u16 {
    fn from(encoding: Encoding) -> u16 {
        encoding as u16
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Encoding::Postcard => write!(f, "postcard"),
            Encoding::Json => write!(f, "json"),
            Encoding::Raw => write!(f, "raw"),
        }
    }
}

/// Error when converting from an unknown u16 encoding value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownEncoding(pub u16);

impl fmt::Display for UnknownEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown encoding: {}", self.0)
    }
}

impl std::error::Error for UnknownEncoding {}

/// Codec trait for message serialization.
///
/// Implementations provide encoding/decoding for specific serialization formats.
/// Each codec declares its encoding type and associated error types.
pub trait Codec {
    /// The encoding type this codec implements.
    const ENCODING: Encoding;

    /// Error type returned by encode operations.
    type EncodeError: std::error::Error;

    /// Error type returned by decode operations.
    type DecodeError: std::error::Error;

    /// Encode a value into bytes.
    fn encode<T: Serialize>(val: &T) -> Result<Vec<u8>, Self::EncodeError>;

    /// Decode bytes into a value.
    fn decode<T: DeserializeOwned>(buf: &[u8]) -> Result<T, Self::DecodeError>;
}

/// Postcard codec: compact binary format using postcard serialization.
///
/// This is the default codec for control messages. It provides:
/// - Compact binary encoding (smaller than JSON)
/// - No schema evolution (breaking changes require version bumps)
/// - Fast encoding/decoding
/// - Deterministic output
pub struct PostcardCodec;

impl Codec for PostcardCodec {
    const ENCODING: Encoding = Encoding::Postcard;
    type EncodeError = postcard::Error;
    type DecodeError = postcard::Error;

    fn encode<T: Serialize>(val: &T) -> Result<Vec<u8>, Self::EncodeError> {
        postcard::to_allocvec(val)
    }

    fn decode<T: DeserializeOwned>(buf: &[u8]) -> Result<T, Self::DecodeError> {
        postcard::from_bytes(buf)
    }
}

/// JSON codec: human-readable format using serde_json.
///
/// This codec is useful for:
/// - Debugging (messages can be inspected as text)
/// - Tooling (external tools can parse JSON)
/// - Interoperability (JSON is universally supported)
///
/// Trade-offs:
/// - Larger message size than binary formats
/// - Slower encoding/decoding than binary formats
pub struct JsonCodec;

impl Codec for JsonCodec {
    const ENCODING: Encoding = Encoding::Json;
    type EncodeError = serde_json::Error;
    type DecodeError = serde_json::Error;

    fn encode<T: Serialize>(val: &T) -> Result<Vec<u8>, Self::EncodeError> {
        serde_json::to_vec(val)
    }

    fn decode<T: DeserializeOwned>(buf: &[u8]) -> Result<T, Self::DecodeError> {
        serde_json::from_slice(buf)
    }
}

/// Raw codec: no serialization, passes bytes through as-is.
///
/// This codec is used for pre-serialized data or binary payloads.
/// It only works with `Vec<u8>` and `&[u8]` types.
///
/// Note: This codec cannot serialize/deserialize arbitrary types.
/// Attempting to use it with non-byte types will result in errors.
pub struct RawCodec;

/// Error type for RawCodec operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCodecError {
    message: &'static str,
}

impl RawCodecError {
    fn new(message: &'static str) -> Self {
        RawCodecError { message }
    }
}

impl fmt::Display for RawCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "raw codec error: {}", self.message)
    }
}

impl std::error::Error for RawCodecError {}

impl Codec for RawCodec {
    const ENCODING: Encoding = Encoding::Raw;
    type EncodeError = RawCodecError;
    type DecodeError = RawCodecError;

    fn encode<T: Serialize>(_val: &T) -> Result<Vec<u8>, Self::EncodeError> {
        // Raw codec doesn't support serialization of arbitrary types
        Err(RawCodecError::new(
            "raw codec only supports Vec<u8>, use to_bytes() instead",
        ))
    }

    fn decode<T: DeserializeOwned>(_buf: &[u8]) -> Result<T, Self::DecodeError> {
        // Raw codec doesn't support deserialization of arbitrary types
        Err(RawCodecError::new(
            "raw codec only supports Vec<u8>, use from_bytes() instead",
        ))
    }
}

impl RawCodec {
    /// Convert bytes to Vec<u8> (no-op, just clones).
    pub fn to_bytes(data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }

    /// Convert bytes from Vec<u8> (no-op, just returns the input).
    pub fn from_bytes(data: Vec<u8>) -> Vec<u8> {
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct TestMessage {
        id: u32,
        name: String,
    }

    #[test]
    fn encoding_try_from_u16() {
        assert_eq!(Encoding::try_from(1).unwrap(), Encoding::Postcard);
        assert_eq!(Encoding::try_from(2).unwrap(), Encoding::Json);
        assert_eq!(Encoding::try_from(3).unwrap(), Encoding::Raw);
        assert_eq!(Encoding::try_from(99), Err(UnknownEncoding(99)));
    }

    #[test]
    fn encoding_to_u16() {
        assert_eq!(u16::from(Encoding::Postcard), 1);
        assert_eq!(u16::from(Encoding::Json), 2);
        assert_eq!(u16::from(Encoding::Raw), 3);
    }

    #[test]
    fn encoding_roundtrip() {
        let encodings = [Encoding::Postcard, Encoding::Json, Encoding::Raw];
        for &encoding in &encodings {
            let val = u16::from(encoding);
            let roundtrip = Encoding::try_from(val).unwrap();
            assert_eq!(encoding, roundtrip);
        }
    }

    #[test]
    fn encoding_display() {
        assert_eq!(format!("{}", Encoding::Postcard), "postcard");
        assert_eq!(format!("{}", Encoding::Json), "json");
        assert_eq!(format!("{}", Encoding::Raw), "raw");
    }

    #[test]
    fn unknown_encoding_display() {
        let err = UnknownEncoding(42);
        let s = format!("{}", err);
        assert!(s.contains("42"));
    }

    #[test]
    fn postcard_codec_roundtrip() {
        let msg = TestMessage {
            id: 42,
            name: "test".to_string(),
        };

        let encoded = PostcardCodec::encode(&msg).unwrap();
        let decoded: TestMessage = PostcardCodec::decode(&encoded).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn postcard_codec_encoding_type() {
        assert_eq!(PostcardCodec::ENCODING, Encoding::Postcard);
    }

    #[test]
    fn postcard_codec_invalid_data() {
        let bad_data = vec![0xFF, 0xFF, 0xFF];
        let result: Result<TestMessage, _> = PostcardCodec::decode(&bad_data);
        assert!(result.is_err());
    }

    #[test]
    fn json_codec_roundtrip() {
        let msg = TestMessage {
            id: 42,
            name: "test".to_string(),
        };

        let encoded = JsonCodec::encode(&msg).unwrap();
        let decoded: TestMessage = JsonCodec::decode(&encoded).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn json_codec_encoding_type() {
        assert_eq!(JsonCodec::ENCODING, Encoding::Json);
    }

    #[test]
    fn json_codec_human_readable() {
        let msg = TestMessage {
            id: 42,
            name: "test".to_string(),
        };

        let encoded = JsonCodec::encode(&msg).unwrap();
        let json_str = String::from_utf8(encoded).unwrap();

        // JSON should be human-readable
        assert!(json_str.contains("42"));
        assert!(json_str.contains("test"));
    }

    #[test]
    fn json_codec_invalid_data() {
        let bad_data = b"not valid json {";
        let result: Result<TestMessage, _> = JsonCodec::decode(bad_data);
        assert!(result.is_err());
    }

    #[test]
    fn raw_codec_encoding_type() {
        assert_eq!(RawCodec::ENCODING, Encoding::Raw);
    }

    #[test]
    fn raw_codec_to_bytes() {
        let data = b"hello world";
        let result = RawCodec::to_bytes(data);
        assert_eq!(result, data);
    }

    #[test]
    fn raw_codec_from_bytes() {
        let data = vec![1, 2, 3, 4, 5];
        let result = RawCodec::from_bytes(data.clone());
        assert_eq!(result, data);
    }

    #[test]
    fn raw_codec_encode_fails() {
        let msg = TestMessage {
            id: 42,
            name: "test".to_string(),
        };
        let result = RawCodec::encode(&msg);
        assert!(result.is_err());
    }

    #[test]
    fn raw_codec_decode_fails() {
        let data = vec![1, 2, 3];
        let result: Result<TestMessage, _> = RawCodec::decode(&data);
        assert!(result.is_err());
    }

    #[test]
    fn raw_codec_error_display() {
        let err = RawCodecError::new("test error");
        let s = format!("{}", err);
        assert!(s.contains("test error"));
    }

    #[test]
    fn postcard_vs_json_size() {
        let msg = TestMessage {
            id: 42,
            name: "test".to_string(),
        };

        let postcard_encoded = PostcardCodec::encode(&msg).unwrap();
        let json_encoded = JsonCodec::encode(&msg).unwrap();

        // Postcard should be more compact than JSON
        assert!(postcard_encoded.len() < json_encoded.len());
    }
}
