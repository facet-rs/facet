//! HTTP header to roam metadata translation.
//!
//! r[bridge.nonce.backend]
//! Nonce deduplication is performed by the backend roam service, not the bridge.
//! The bridge is stateless with respect to nonces - it simply passes them through.

use std::collections::HashMap;

/// Request metadata extracted from HTTP headers.
///
/// r[bridge.request.metadata]
/// r[bridge.request.metadata.wellknown]
/// r[bridge.request.nonce]
#[derive(Debug, Default, Clone)]
pub struct BridgeMetadata {
    /// Key-value pairs of metadata.
    entries: HashMap<String, MetadataValue>,
}

/// A metadata value (string or bytes).
#[derive(Debug, Clone)]
pub enum MetadataValue {
    /// String value.
    String(String),
    /// Binary value (e.g., decoded nonce).
    Bytes(Vec<u8>),
}

impl BridgeMetadata {
    /// Create empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a string value.
    pub fn insert_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries
            .insert(key.into(), MetadataValue::String(value.into()));
    }

    /// Insert a bytes value.
    pub fn insert_bytes(&mut self, key: impl Into<String>, value: Vec<u8>) {
        self.entries.insert(key.into(), MetadataValue::Bytes(value));
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&MetadataValue> {
        self.entries.get(key)
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &MetadataValue)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Convert to roam wire metadata format.
    pub fn to_wire_metadata(&self) -> roam_wire::Metadata {
        use roam_wire::metadata_flags;

        self.entries
            .iter()
            .map(|(k, v)| {
                let wire_value = match v {
                    MetadataValue::String(s) => roam_wire::MetadataValue::String(s.clone()),
                    MetadataValue::Bytes(b) => roam_wire::MetadataValue::Bytes(b.clone()),
                };
                // r[call.metadata.flags] - Mark authorization as sensitive
                let flags = if k == "authorization" {
                    metadata_flags::SENSITIVE
                } else {
                    metadata_flags::NONE
                };
                (k.clone(), wire_value, flags)
            })
            .collect()
    }
}

/// Well-known HTTP headers that pass through without prefix transformation.
///
/// r[bridge.request.metadata.wellknown]
const WELLKNOWN_HEADERS: &[&str] = &["traceparent", "tracestate", "authorization"];

/// Extract metadata from HTTP headers.
///
/// r[bridge.request.metadata]
/// r[bridge.request.metadata.wellknown]
/// r[bridge.request.nonce]
pub fn extract_metadata(headers: &http::HeaderMap) -> Result<BridgeMetadata, crate::BridgeError> {
    let mut metadata = BridgeMetadata::new();

    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        let value_str = value
            .to_str()
            .map_err(|_| crate::BridgeError::bad_request("Invalid header value encoding"))?;

        // r[bridge.request.metadata.wellknown]
        // Well-known headers pass through without prefix
        if WELLKNOWN_HEADERS.contains(&name_str) {
            metadata.insert_string(name_str, value_str);
            continue;
        }

        // r[bridge.request.nonce]
        // r[bridge.nonce.passthrough]
        // Special handling for Roam-Nonce: base64 decode to 16 bytes,
        // then include as roam-nonce in the roam request metadata
        if name_str.eq_ignore_ascii_case("roam-nonce") {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(value_str)
                .map_err(|_| {
                    crate::BridgeError::bad_request("Invalid Roam-Nonce: not valid base64")
                })?;
            if bytes.len() != 16 {
                return Err(crate::BridgeError::bad_request(format!(
                    "Invalid Roam-Nonce: expected 16 bytes, got {}",
                    bytes.len()
                )));
            }
            metadata.insert_bytes("roam-nonce", bytes);
            continue;
        }

        // r[bridge.request.metadata]
        // Roam-{key} headers map to metadata key {key}
        if let Some(key) = name_str.strip_prefix("roam-") {
            metadata.insert_string(key, value_str);
        }
        // Also handle case-insensitive "Roam-" prefix
        else if name_str.len() > 5 && name_str[..5].eq_ignore_ascii_case("roam-") {
            let key = &name_str[5..];
            metadata.insert_string(key, value_str);
        }
    }

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;

    #[test]
    fn test_wellknown_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", "00-abc-def-01".parse().unwrap());
        headers.insert("authorization", "Bearer token".parse().unwrap());

        let metadata = extract_metadata(&headers).unwrap();

        assert!(matches!(
            metadata.get("traceparent"),
            Some(MetadataValue::String(s)) if s == "00-abc-def-01"
        ));
        assert!(matches!(
            metadata.get("authorization"),
            Some(MetadataValue::String(s)) if s == "Bearer token"
        ));
    }

    #[test]
    fn test_roam_prefixed_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("roam-request-id", "abc123".parse().unwrap());
        headers.insert("Roam-Custom", "value".parse().unwrap());

        let metadata = extract_metadata(&headers).unwrap();

        assert!(matches!(
            metadata.get("request-id"),
            Some(MetadataValue::String(s)) if s == "abc123"
        ));
    }

    #[test]
    fn test_nonce_decoding() {
        use base64::Engine;
        let nonce = [0u8; 16];
        let encoded = base64::engine::general_purpose::STANDARD.encode(nonce);

        let mut headers = HeaderMap::new();
        headers.insert("roam-nonce", encoded.parse().unwrap());

        let metadata = extract_metadata(&headers).unwrap();

        assert!(matches!(
            metadata.get("roam-nonce"),
            Some(MetadataValue::Bytes(b)) if b.len() == 16
        ));
    }

    #[test]
    fn test_invalid_nonce_length() {
        use base64::Engine;
        let nonce = [0u8; 8]; // Wrong length
        let encoded = base64::engine::general_purpose::STANDARD.encode(nonce);

        let mut headers = HeaderMap::new();
        headers.insert("roam-nonce", encoded.parse().unwrap());

        let result = extract_metadata(&headers);
        assert!(result.is_err());
    }
}
