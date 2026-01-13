//! WebSocket message types for the roam bridge protocol.
//!
//! r[bridge.ws.message-format] - Each WebSocket message is a JSON object with a `type` field.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Incoming messages from the client.
///
/// r[bridge.ws.message-format]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    /// r[bridge.ws.request] - Initiates an RPC call.
    Request {
        /// Client-assigned request ID for correlation.
        id: u64,
        /// Service name.
        service: String,
        /// Method name.
        method: String,
        /// JSON-encoded arguments as an array.
        args: serde_json::Value,
        /// Optional request metadata.
        #[serde(default)]
        metadata: HashMap<String, serde_json::Value>,
    },

    /// r[bridge.ws.data] - Sends a value on a channel (Tx direction).
    Data {
        /// Channel ID.
        channel: u64,
        /// The value to send.
        value: serde_json::Value,
    },

    /// r[bridge.ws.close] - Signals end of a Tx channel.
    Close {
        /// Channel ID to close.
        channel: u64,
    },

    /// r[bridge.ws.reset] - Forcefully terminates a channel.
    Reset {
        /// Channel ID to reset.
        channel: u64,
    },

    /// r[bridge.ws.credit] - Grants flow control credit.
    Credit {
        /// Channel ID.
        channel: u64,
        /// Credit in bytes.
        bytes: u64,
    },

    /// r[bridge.ws.cancel] - Requests cancellation of an in-flight RPC.
    Cancel {
        /// Request ID to cancel.
        id: u64,
    },
}

/// Outgoing messages to the client.
///
/// r[bridge.ws.message-format]
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
#[allow(dead_code)]
pub enum ServerMessage {
    /// r[bridge.ws.response] - Completes an RPC call with success.
    #[serde(rename = "response")]
    ResponseSuccess {
        /// Request ID being responded to.
        id: u64,
        /// The result value.
        result: serde_json::Value,
    },

    /// r[bridge.ws.response] - Completes an RPC call with protocol error.
    #[serde(rename = "response")]
    ResponseProtocolError {
        /// Request ID being responded to.
        id: u64,
        /// Error type: "unknown_method", "invalid_payload", "cancelled".
        error: &'static str,
    },

    /// r[bridge.ws.response] - Completes an RPC call with user error.
    #[serde(rename = "response")]
    ResponseUserError {
        /// Request ID being responded to.
        id: u64,
        /// Always "user".
        error: &'static str,
        /// The user error value.
        value: serde_json::Value,
    },

    /// r[bridge.ws.data] - Sends a value on a channel (Rx direction).
    Data {
        /// Channel ID.
        channel: u64,
        /// The value being sent.
        value: serde_json::Value,
    },

    /// r[bridge.ws.reset] - Forcefully terminates a channel.
    Reset {
        /// Channel ID.
        channel: u64,
    },

    /// r[bridge.ws.credit] - Grants flow control credit.
    Credit {
        /// Channel ID.
        channel: u64,
        /// Credit in bytes.
        bytes: u64,
    },

    /// r[bridge.ws.goodbye] - Signals connection termination.
    Goodbye {
        /// Reason for termination (rule ID).
        reason: String,
    },
}

#[allow(dead_code)]
impl ServerMessage {
    /// Create a success response.
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        ServerMessage::ResponseSuccess { id, result }
    }

    /// Create a protocol error response.
    pub fn protocol_error(id: u64, error: &'static str) -> Self {
        ServerMessage::ResponseProtocolError { id, error }
    }

    /// Create a user error response.
    pub fn user_error(id: u64, value: serde_json::Value) -> Self {
        ServerMessage::ResponseUserError {
            id,
            error: "user",
            value,
        }
    }

    /// Create a goodbye message.
    pub fn goodbye(reason: impl Into<String>) -> Self {
        ServerMessage::Goodbye {
            reason: reason.into(),
        }
    }

    /// Create a data message.
    pub fn data(channel: u64, value: serde_json::Value) -> Self {
        ServerMessage::Data { channel, value }
    }

    /// Create a credit message.
    pub fn credit(channel: u64, bytes: u64) -> Self {
        ServerMessage::Credit { channel, bytes }
    }
}

/// The expected WebSocket subprotocol.
///
/// r[bridge.ws.subprotocol]
pub const WS_SUBPROTOCOL: &str = "roam-bridge.v1";
