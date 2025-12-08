//! Message header (in payload).

use crate::Encoding;

/// Message header at the start of each payload.
///
/// Layout: `payload = [MsgHeader][metadata...][body...]`
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MsgHeader {
    /// Message format version.
    pub version: u16,
    /// Total header size (including this struct + metadata).
    pub header_len: u16,
    /// Body encoding (Postcard, Json, Raw).
    pub encoding: u16,
    /// Header flags (compression, etc.).
    pub flags: u16,
    /// Reply-to: msg_id of request.
    pub correlation_id: u64,
    /// Absolute deadline (nanos since epoch, 0 = none).
    pub deadline_ns: u64,
}

/// Current message header version.
pub const MSG_HEADER_VERSION: u16 = 1;

/// Size of MsgHeader in bytes.
pub const MSG_HEADER_SIZE: usize = core::mem::size_of::<MsgHeader>();

const _: () = assert!(MSG_HEADER_SIZE == 24);

impl MsgHeader {
    /// Create a new header with default values.
    pub const fn new() -> Self {
        Self {
            version: MSG_HEADER_VERSION,
            header_len: MSG_HEADER_SIZE as u16,
            encoding: Encoding::Postcard as u16,
            flags: 0,
            correlation_id: 0,
            deadline_ns: 0,
        }
    }

    /// Get the encoding as an enum.
    pub fn encoding(&self) -> Option<Encoding> {
        Encoding::from_u16(self.encoding)
    }

    /// Set the encoding.
    pub fn set_encoding(&mut self, encoding: Encoding) {
        self.encoding = encoding as u16;
    }
}

impl Default for MsgHeader {
    fn default() -> Self {
        Self::new()
    }
}
