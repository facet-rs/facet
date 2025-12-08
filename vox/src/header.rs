// src/header.rs

/// Fixed size of the message header
pub const MSG_HEADER_FIXED_SIZE: usize = 24;

/// Maximum metadata limits
pub const MAX_METADATA_PAIRS: usize = 32;
pub const MAX_METADATA_KEY_LEN: usize = 64;
pub const MAX_METADATA_VALUE_LEN: usize = 4096;

/// Body encoding format
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// postcard via facet-postcard - default for control messages
    Postcard = 1,
    /// JSON - for debugging, external tooling
    Json = 2,
    /// Raw bytes - app-defined, no schema
    Raw = 3,
}

impl TryFrom<u16> for Encoding {
    type Error = ();

    fn try_from(v: u16) -> Result<Self, ()> {
        match v {
            1 => Ok(Encoding::Postcard),
            2 => Ok(Encoding::Json),
            3 => Ok(Encoding::Raw),
            _ => Err(()),
        }
    }
}

/// Message header (safe representation).
#[derive(Debug, Clone, PartialEq)]
pub struct MsgHeader {
    pub version: u16,
    pub encoding: Encoding,
    pub flags: u16,
    pub correlation_id: u64,
    pub deadline_ns: u64,
    pub metadata: Metadata,
}

impl Default for MsgHeader {
    fn default() -> Self {
        MsgHeader {
            version: 1,
            encoding: Encoding::Postcard,
            flags: 0,
            correlation_id: 0,
            deadline_ns: 0,
            metadata: Metadata::default(),
        }
    }
}

/// Metadata key-value pairs with enforced limits.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    pairs: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    TooShort,
    InvalidHeaderLen,
    MetadataTooLarge,
    InvalidEncoding,
    KeyTooLong,
    ValueTooLong,
    TooManyPairs,
    InvalidMetadata,
}

impl MsgHeader {
    /// Create a new header with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the encoding
    pub fn with_encoding(mut self, encoding: Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Set correlation ID (for request-response matching)
    pub fn with_correlation_id(mut self, id: u64) -> Self {
        self.correlation_id = id;
        self
    }

    /// Set deadline (absolute nanoseconds since epoch)
    pub fn with_deadline(mut self, deadline_ns: u64) -> Self {
        self.deadline_ns = deadline_ns;
        self
    }

    /// Encode header into buffer. Returns total header_len.
    pub fn encode_into(&self, buf: &mut [u8]) -> Result<usize, HeaderError> {
        if buf.len() < MSG_HEADER_FIXED_SIZE {
            return Err(HeaderError::TooShort);
        }

        // Encode metadata to get length
        let metadata_bytes = self.metadata.encode();
        let header_len = MSG_HEADER_FIXED_SIZE + metadata_bytes.len();

        if buf.len() < header_len {
            return Err(HeaderError::TooShort);
        }

        // Write fixed header (24 bytes)
        buf[0..2].copy_from_slice(&self.version.to_le_bytes());
        buf[2..4].copy_from_slice(&(header_len as u16).to_le_bytes());
        buf[4..6].copy_from_slice(&(self.encoding as u16).to_le_bytes());
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..16].copy_from_slice(&self.correlation_id.to_le_bytes());
        buf[16..24].copy_from_slice(&self.deadline_ns.to_le_bytes());

        // Write metadata
        if !metadata_bytes.is_empty() {
            buf[24..header_len].copy_from_slice(&metadata_bytes);
        }

        Ok(header_len)
    }

    /// Decode header from buffer. Returns (header, header_len).
    pub fn decode_from(buf: &[u8]) -> Result<(Self, usize), HeaderError> {
        if buf.len() < MSG_HEADER_FIXED_SIZE {
            return Err(HeaderError::TooShort);
        }

        let version = u16::from_le_bytes([buf[0], buf[1]]);
        let header_len = u16::from_le_bytes([buf[2], buf[3]]) as usize;
        let encoding_raw = u16::from_le_bytes([buf[4], buf[5]]);
        let flags = u16::from_le_bytes([buf[6], buf[7]]);
        let correlation_id = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let deadline_ns = u64::from_le_bytes(buf[16..24].try_into().unwrap());

        // Validate header_len
        if header_len < MSG_HEADER_FIXED_SIZE || header_len > buf.len() {
            return Err(HeaderError::InvalidHeaderLen);
        }

        let encoding = Encoding::try_from(encoding_raw)
            .map_err(|_| HeaderError::InvalidEncoding)?;

        let metadata = if header_len > MSG_HEADER_FIXED_SIZE {
            Metadata::decode(&buf[MSG_HEADER_FIXED_SIZE..header_len])?
        } else {
            Metadata::default()
        };

        Ok((
            MsgHeader {
                version,
                encoding,
                flags,
                correlation_id,
                deadline_ns,
                metadata,
            },
            header_len,
        ))
    }

    /// Get body slice from payload buffer (after header).
    pub fn body_from_payload<'a>(&self, payload: &'a [u8], header_len: usize) -> &'a [u8] {
        &payload[header_len..]
    }

    /// Calculate encoded size
    pub fn encoded_size(&self) -> usize {
        MSG_HEADER_FIXED_SIZE + self.metadata.encoded_size()
    }
}

impl Metadata {
    /// Create empty metadata
    pub fn new() -> Self {
        Metadata { pairs: Vec::new() }
    }

    /// Insert a key-value pair
    pub fn insert(&mut self, key: String, value: Vec<u8>) -> Result<(), HeaderError> {
        if self.pairs.len() >= MAX_METADATA_PAIRS {
            return Err(HeaderError::TooManyPairs);
        }
        if key.len() > MAX_METADATA_KEY_LEN {
            return Err(HeaderError::KeyTooLong);
        }
        if value.len() > MAX_METADATA_VALUE_LEN {
            return Err(HeaderError::ValueTooLong);
        }
        self.pairs.push((key, value));
        Ok(())
    }

    /// Get a value by key
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.pairs.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }

    /// Iterate over all pairs
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Number of pairs
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Encode metadata to bytes
    /// Format: [num_pairs: u16] [key_len: u16, key: bytes, val_len: u32, val: bytes]*
    fn encode(&self) -> Vec<u8> {
        if self.pairs.is_empty() {
            return Vec::new();
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(&(self.pairs.len() as u16).to_le_bytes());

        for (key, value) in &self.pairs {
            buf.extend_from_slice(&(key.len() as u16).to_le_bytes());
            buf.extend_from_slice(key.as_bytes());
            buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
            buf.extend_from_slice(value);
        }

        buf
    }

    /// Decode metadata from bytes
    fn decode(buf: &[u8]) -> Result<Self, HeaderError> {
        if buf.len() < 2 {
            return Err(HeaderError::InvalidMetadata);
        }

        let num_pairs = u16::from_le_bytes([buf[0], buf[1]]) as usize;
        if num_pairs > MAX_METADATA_PAIRS {
            return Err(HeaderError::TooManyPairs);
        }

        let mut pairs = Vec::with_capacity(num_pairs);
        let mut pos = 2;

        for _ in 0..num_pairs {
            // Read key length
            if pos + 2 > buf.len() {
                return Err(HeaderError::InvalidMetadata);
            }
            let key_len = u16::from_le_bytes([buf[pos], buf[pos + 1]]) as usize;
            pos += 2;

            if key_len > MAX_METADATA_KEY_LEN {
                return Err(HeaderError::KeyTooLong);
            }

            // Read key
            if pos + key_len > buf.len() {
                return Err(HeaderError::InvalidMetadata);
            }
            let key = String::from_utf8(buf[pos..pos + key_len].to_vec())
                .map_err(|_| HeaderError::InvalidMetadata)?;
            pos += key_len;

            // Read value length
            if pos + 4 > buf.len() {
                return Err(HeaderError::InvalidMetadata);
            }
            let val_len = u32::from_le_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as usize;
            pos += 4;

            if val_len > MAX_METADATA_VALUE_LEN {
                return Err(HeaderError::ValueTooLong);
            }

            // Read value
            if pos + val_len > buf.len() {
                return Err(HeaderError::InvalidMetadata);
            }
            let value = buf[pos..pos + val_len].to_vec();
            pos += val_len;

            pairs.push((key, value));
        }

        Ok(Metadata { pairs })
    }

    /// Calculate encoded size
    fn encoded_size(&self) -> usize {
        if self.pairs.is_empty() {
            return 0;
        }

        let mut size = 2; // num_pairs
        for (key, value) in &self.pairs {
            size += 2 + key.len() + 4 + value.len();
        }
        size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip_no_metadata() {
        let header = MsgHeader::new()
            .with_encoding(Encoding::Json)
            .with_correlation_id(12345)
            .with_deadline(999999);

        let mut buf = [0u8; 256];
        let len = header.encode_into(&mut buf).unwrap();

        assert_eq!(len, MSG_HEADER_FIXED_SIZE);

        let (decoded, decoded_len) = MsgHeader::decode_from(&buf).unwrap();
        assert_eq!(decoded_len, len);
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.encoding, Encoding::Json);
        assert_eq!(decoded.correlation_id, 12345);
        assert_eq!(decoded.deadline_ns, 999999);
    }

    #[test]
    fn header_roundtrip_with_metadata() {
        let mut header = MsgHeader::new();
        header.metadata.insert("trace-id".to_string(), b"abc123".to_vec()).unwrap();
        header.metadata.insert("user".to_string(), b"alice".to_vec()).unwrap();

        let mut buf = [0u8; 256];
        let len = header.encode_into(&mut buf).unwrap();

        assert!(len > MSG_HEADER_FIXED_SIZE);

        let (decoded, decoded_len) = MsgHeader::decode_from(&buf).unwrap();
        assert_eq!(decoded_len, len);
        assert_eq!(decoded.metadata.get("trace-id"), Some(b"abc123".as_slice()));
        assert_eq!(decoded.metadata.get("user"), Some(b"alice".as_slice()));
    }

    #[test]
    fn metadata_limits() {
        let mut metadata = Metadata::new();

        // Key too long
        let long_key = "x".repeat(MAX_METADATA_KEY_LEN + 1);
        assert_eq!(
            metadata.insert(long_key, vec![]),
            Err(HeaderError::KeyTooLong)
        );

        // Value too long
        let long_value = vec![0u8; MAX_METADATA_VALUE_LEN + 1];
        assert_eq!(
            metadata.insert("key".to_string(), long_value),
            Err(HeaderError::ValueTooLong)
        );
    }

    #[test]
    fn header_too_short() {
        let buf = [0u8; 10];
        assert_eq!(MsgHeader::decode_from(&buf), Err(HeaderError::TooShort));
    }

    #[test]
    fn invalid_encoding() {
        let mut buf = [0u8; 24];
        buf[4..6].copy_from_slice(&99u16.to_le_bytes()); // Invalid encoding
        buf[2..4].copy_from_slice(&24u16.to_le_bytes()); // header_len = 24

        assert_eq!(MsgHeader::decode_from(&buf), Err(HeaderError::InvalidEncoding));
    }

    #[test]
    fn encoding_roundtrip() {
        for encoding in [Encoding::Postcard, Encoding::Json, Encoding::Raw] {
            let header = MsgHeader::new().with_encoding(encoding);
            let mut buf = [0u8; 64];
            header.encode_into(&mut buf).unwrap();
            let (decoded, _) = MsgHeader::decode_from(&buf).unwrap();
            assert_eq!(decoded.encoding, encoding);
        }
    }
}
