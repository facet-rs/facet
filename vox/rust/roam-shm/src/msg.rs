//! Message type constants.
//!
//! Defines the message type values used in the `MsgDesc.msg_type` field.

/// Message type constants.
///
/// shm[impl shm.desc.msg-type]
pub mod msg_type {
    /// Request message (carries request_id and method_id)
    pub const REQUEST: u8 = 1;
    /// Response message (carries request_id)
    pub const RESPONSE: u8 = 2;
    /// Cancel message (carries request_id)
    pub const CANCEL: u8 = 3;
    /// Data message (carries channel_id)
    pub const DATA: u8 = 4;
    /// Close message (carries channel_id)
    pub const CLOSE: u8 = 5;
    /// Reset message (carries channel_id)
    pub const RESET: u8 = 6;
    /// Goodbye message (id field unused)
    pub const GOODBYE: u8 = 7;
    /// Connect message (carries request_id for virtual connection)
    pub const CONNECT: u8 = 8;
    /// Accept message (carries request_id, assigns conn_id)
    pub const ACCEPT: u8 = 9;
    /// Reject message (carries request_id)
    pub const REJECT: u8 = 10;
}

/// Check if a message type uses request_id in the id field.
#[inline]
pub const fn uses_request_id(msg_type: u8) -> bool {
    matches!(
        msg_type,
        msg_type::REQUEST
            | msg_type::RESPONSE
            | msg_type::CANCEL
            | msg_type::CONNECT
            | msg_type::ACCEPT
            | msg_type::REJECT
    )
}

/// Check if a message type uses channel_id in the id field.
#[inline]
pub const fn uses_channel_id(msg_type: u8) -> bool {
    matches!(msg_type, msg_type::DATA | msg_type::CLOSE | msg_type::RESET)
}

/// Message type name for debugging.
pub const fn msg_type_name(msg_type: u8) -> &'static str {
    match msg_type {
        msg_type::REQUEST => "Request",
        msg_type::RESPONSE => "Response",
        msg_type::CANCEL => "Cancel",
        msg_type::DATA => "Data",
        msg_type::CLOSE => "Close",
        msg_type::RESET => "Reset",
        msg_type::GOODBYE => "Goodbye",
        msg_type::CONNECT => "Connect",
        msg_type::ACCEPT => "Accept",
        msg_type::REJECT => "Reject",
        _ => "Unknown",
    }
}

/// Decoded SHM message — v2-native representation.
///
/// Carries just the fields needed for BipBuffer frame encoding:
/// msg_type, id, method_id, and payload bytes.
#[derive(Debug, Clone)]
pub struct ShmMsg {
    pub msg_type: u8,
    pub id: u32,
    pub method_id: u64,
    pub payload: Vec<u8>,
}

impl ShmMsg {
    /// Create a new ShmMsg.
    #[inline]
    pub fn new(msg_type: u8, id: u32, method_id: u64, payload: Vec<u8>) -> Self {
        Self {
            msg_type,
            id,
            method_id,
            payload,
        }
    }

    /// Get payload bytes.
    #[inline]
    pub fn payload_bytes(&self) -> &[u8] {
        &self.payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_type_classification() {
        // shm[verify shm.desc.msg-type]
        assert!(uses_request_id(msg_type::REQUEST));
        assert!(uses_request_id(msg_type::RESPONSE));
        assert!(uses_request_id(msg_type::CANCEL));

        assert!(uses_channel_id(msg_type::DATA));
        assert!(uses_channel_id(msg_type::CLOSE));
        assert!(uses_channel_id(msg_type::RESET));

        assert!(!uses_request_id(msg_type::GOODBYE));
        assert!(!uses_channel_id(msg_type::GOODBYE));
    }
}
