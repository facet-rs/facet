//! Transport abstraction for SHM.
//!
//! This module provides the bridge between roam's `MessageTransport` trait
//! and the SHM v2-native `ShmMsg` type. It handles:
//!
//! - Converting between `roam_wire::Message` and `ShmMsg`
//! - Encoding/decoding metadata alongside payload
//! - Async wrappers for the synchronous SHM operations
//!
//! shm[impl shm.scope]

use std::io;
use std::time::Duration;

use roam_wire::Message;

use crate::guest::{SendError, ShmGuest};
use crate::msg::{ShmMsg, msg_type};

/// Conversion error when mapping between Message and ShmMsg.
#[derive(Debug)]
pub enum ConvertError {
    /// Unknown message type
    UnknownMsgType(u8),
    /// Payload decode error
    DecodeError(String),
    /// Hello messages not supported in SHM
    HelloNotSupported,
    /// Credit messages not supported in SHM (flow control via channel table)
    CreditNotSupported,
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::UnknownMsgType(t) => write!(f, "unknown message type: {}", t),
            ConvertError::DecodeError(e) => write!(f, "decode error: {}", e),
            ConvertError::HelloNotSupported => write!(f, "Hello messages not supported in SHM"),
            ConvertError::CreditNotSupported => {
                write!(
                    f,
                    "Credit messages not supported in SHM (use channel table)"
                )
            }
        }
    }
}

impl std::error::Error for ConvertError {}

/// Convert a `roam_wire::Message` to an `ShmMsg`.
///
/// shm[impl shm.metadata.in-payload]
///
/// For Request and Response, metadata is prepended to the payload and
/// encoded together using postcard.
pub fn message_to_shm_msg(msg: &Message) -> Result<ShmMsg, ConvertError> {
    match msg {
        Message::Hello(_) => {
            // shm[impl shm.handshake]
            // SHM doesn't use Hello messages - handshake is implicit via segment header
            Err(ConvertError::HelloNotSupported)
        }

        Message::Goodbye { reason, .. } => Ok(ShmMsg::new(
            msg_type::GOODBYE,
            0,
            0,
            reason.as_bytes().to_vec(),
        )),

        Message::Request {
            conn_id,
            request_id,
            method_id,
            metadata,
            channels,
            payload,
        } => {
            // shm[impl shm.metadata.in-payload]
            let combined = encode_request_payload(*conn_id, metadata, channels, payload);
            Ok(ShmMsg::new(
                msg_type::REQUEST,
                *request_id as u32,
                *method_id,
                combined,
            ))
        }

        Message::Response {
            conn_id,
            request_id,
            metadata,
            channels,
            payload,
        } => {
            // shm[impl shm.metadata.in-payload]
            let combined = encode_response_payload(*conn_id, metadata, channels, payload);
            Ok(ShmMsg::new(
                msg_type::RESPONSE,
                *request_id as u32,
                0,
                combined,
            ))
        }

        Message::Cancel {
            conn_id,
            request_id,
        } => Ok(ShmMsg::new(
            msg_type::CANCEL,
            *request_id as u32,
            0,
            conn_id.0.to_le_bytes().to_vec(),
        )),

        Message::Data {
            conn_id,
            channel_id,
            payload,
        } => {
            // Prepend conn_id to payload for virtual connection support
            let conn_id_bytes = conn_id.0.to_le_bytes();
            let mut combined = Vec::with_capacity(conn_id_bytes.len() + payload.len());
            combined.extend_from_slice(&conn_id_bytes);
            combined.extend_from_slice(payload);
            Ok(ShmMsg::new(msg_type::DATA, *channel_id as u32, 0, combined))
        }

        Message::Close {
            conn_id,
            channel_id,
        } => Ok(ShmMsg::new(
            msg_type::CLOSE,
            *channel_id as u32,
            0,
            conn_id.0.to_le_bytes().to_vec(),
        )),

        Message::Reset {
            conn_id,
            channel_id,
        } => Ok(ShmMsg::new(
            msg_type::RESET,
            *channel_id as u32,
            0,
            conn_id.0.to_le_bytes().to_vec(),
        )),

        Message::Connect {
            request_id,
            metadata,
        } => {
            let payload_bytes = facet_postcard::to_vec(metadata).unwrap_or_default();
            Ok(ShmMsg::new(
                msg_type::CONNECT,
                *request_id as u32,
                0,
                payload_bytes,
            ))
        }

        Message::Accept {
            request_id,
            conn_id,
            metadata,
        } => {
            let payload_bytes =
                facet_postcard::to_vec(&(conn_id.0, metadata.clone())).unwrap_or_default();
            Ok(ShmMsg::new(
                msg_type::ACCEPT,
                *request_id as u32,
                0,
                payload_bytes,
            ))
        }

        Message::Reject {
            request_id,
            reason,
            metadata,
        } => {
            let payload_bytes =
                facet_postcard::to_vec(&(reason.clone(), metadata.clone())).unwrap_or_default();
            Ok(ShmMsg::new(
                msg_type::REJECT,
                *request_id as u32,
                0,
                payload_bytes,
            ))
        }

        Message::Credit { .. } => {
            // shm[impl shm.flow.no-credit-message]
            // SHM uses channel table for flow control, not Credit messages
            Err(ConvertError::CreditNotSupported)
        }
    }
}

/// Convert an `ShmMsg` to a `roam_wire::Message`.
///
/// shm[impl shm.metadata.in-payload]
pub fn shm_msg_to_message(msg: ShmMsg) -> Result<Message, ConvertError> {
    let payload_bytes = &msg.payload;

    match msg.msg_type {
        msg_type::GOODBYE => {
            let reason = String::from_utf8_lossy(payload_bytes).into_owned();
            let conn_id = roam_wire::ConnectionId::ROOT;
            Ok(Message::Goodbye { conn_id, reason })
        }

        msg_type::REQUEST => {
            let (decoded_conn_id, metadata, channels, payload) =
                decode_request_payload(payload_bytes)
                    .map_err(|e| ConvertError::DecodeError(e.to_string()))?;

            Ok(Message::Request {
                conn_id: decoded_conn_id,
                request_id: msg.id as u64,
                method_id: msg.method_id,
                metadata,
                channels,
                payload,
            })
        }

        msg_type::RESPONSE => {
            let (decoded_conn_id, metadata, channels, payload) =
                decode_response_payload(payload_bytes)
                    .map_err(|e| ConvertError::DecodeError(e.to_string()))?;

            Ok(Message::Response {
                conn_id: decoded_conn_id,
                request_id: msg.id as u64,
                metadata,
                channels,
                payload,
            })
        }

        msg_type::CANCEL => {
            if payload_bytes.len() < 8 {
                return Err(ConvertError::DecodeError(
                    "Cancel payload too short for conn_id".into(),
                ));
            }
            let decoded_conn_id =
                roam_wire::ConnectionId(u64::from_le_bytes(payload_bytes[..8].try_into().unwrap()));
            Ok(Message::Cancel {
                conn_id: decoded_conn_id,
                request_id: msg.id as u64,
            })
        }

        msg_type::DATA => {
            if payload_bytes.len() < 8 {
                return Err(ConvertError::DecodeError(
                    "Data payload too short for conn_id".into(),
                ));
            }
            let decoded_conn_id =
                roam_wire::ConnectionId(u64::from_le_bytes(payload_bytes[..8].try_into().unwrap()));
            Ok(Message::Data {
                conn_id: decoded_conn_id,
                channel_id: msg.id as u64,
                payload: payload_bytes[8..].to_vec(),
            })
        }

        msg_type::CLOSE => {
            if payload_bytes.len() < 8 {
                return Err(ConvertError::DecodeError(
                    "Close payload too short for conn_id".into(),
                ));
            }
            let decoded_conn_id =
                roam_wire::ConnectionId(u64::from_le_bytes(payload_bytes[..8].try_into().unwrap()));
            Ok(Message::Close {
                conn_id: decoded_conn_id,
                channel_id: msg.id as u64,
            })
        }

        msg_type::RESET => {
            if payload_bytes.len() < 8 {
                return Err(ConvertError::DecodeError(
                    "Reset payload too short for conn_id".into(),
                ));
            }
            let decoded_conn_id =
                roam_wire::ConnectionId(u64::from_le_bytes(payload_bytes[..8].try_into().unwrap()));
            Ok(Message::Reset {
                conn_id: decoded_conn_id,
                channel_id: msg.id as u64,
            })
        }

        msg_type::CONNECT => {
            let metadata: roam_wire::Metadata = facet_postcard::from_slice(payload_bytes)
                .map_err(|e| ConvertError::DecodeError(e.to_string()))?;
            Ok(Message::Connect {
                request_id: msg.id as u64,
                metadata,
            })
        }

        msg_type::ACCEPT => {
            let (conn_id_val, metadata): (u64, roam_wire::Metadata) =
                facet_postcard::from_slice(payload_bytes)
                    .map_err(|e| ConvertError::DecodeError(e.to_string()))?;
            Ok(Message::Accept {
                request_id: msg.id as u64,
                conn_id: roam_wire::ConnectionId(conn_id_val),
                metadata,
            })
        }

        msg_type::REJECT => {
            let (reason, metadata): (String, roam_wire::Metadata) =
                facet_postcard::from_slice(payload_bytes)
                    .map_err(|e| ConvertError::DecodeError(e.to_string()))?;
            Ok(Message::Reject {
                request_id: msg.id as u64,
                reason,
                metadata,
            })
        }

        other => Err(ConvertError::UnknownMsgType(other)),
    }
}

/// Combined payload for Request/Response messages.
/// Includes conn_id to support virtual connections over SHM.
#[derive(facet::Facet)]
struct CombinedPayload {
    conn_id: u64,
    metadata: roam_wire::Metadata,
    channels: Vec<u64>,
    payload: Vec<u8>,
}

/// Encode conn_id + metadata + channels + payload for Request messages.
fn encode_request_payload(
    conn_id: roam_wire::ConnectionId,
    metadata: &roam_wire::Metadata,
    channels: &[u64],
    payload: &[u8],
) -> Vec<u8> {
    let combined = CombinedPayload {
        conn_id: conn_id.0,
        metadata: metadata.clone(),
        channels: channels.to_vec(),
        payload: payload.to_vec(),
    };
    let result = facet_postcard::to_vec(&combined).unwrap_or_default();
    tracing::debug!(
        conn_id = conn_id.0,
        channels = ?channels,
        result_len = result.len(),
        "encode_request_payload"
    );
    result
}

/// Encode conn_id + metadata + channels + payload for Response messages.
fn encode_response_payload(
    conn_id: roam_wire::ConnectionId,
    metadata: &roam_wire::Metadata,
    channels: &[u64],
    payload: &[u8],
) -> Vec<u8> {
    let combined = CombinedPayload {
        conn_id: conn_id.0,
        metadata: metadata.clone(),
        channels: channels.to_vec(),
        payload: payload.to_vec(),
    };
    facet_postcard::to_vec(&combined).unwrap_or_default()
}

type DecodedRequestPayloadWithConnId = Result<
    (
        roam_wire::ConnectionId,
        roam_wire::Metadata,
        Vec<u64>,
        Vec<u8>,
    ),
    String,
>;

type DecodedResponsePayloadWithConnId = Result<
    (
        roam_wire::ConnectionId,
        roam_wire::Metadata,
        Vec<u64>,
        Vec<u8>,
    ),
    String,
>;

/// Decode conn_id + metadata + channels + payload for Request messages.
fn decode_request_payload(data: &[u8]) -> DecodedRequestPayloadWithConnId {
    tracing::debug!(data_len = data.len(), "decode_request_payload: input");
    if data.is_empty() {
        tracing::debug!("decode_request_payload: empty data, returning empty");
        return Ok((
            roam_wire::ConnectionId::ROOT,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }
    let combined: CombinedPayload =
        facet_postcard::from_slice(data).map_err(|e| format!("decode error: {}", e))?;
    tracing::debug!(
        conn_id = combined.conn_id,
        channels = ?combined.channels,
        "decode_request_payload: decoded"
    );
    Ok((
        roam_wire::ConnectionId(combined.conn_id),
        combined.metadata,
        combined.channels,
        combined.payload,
    ))
}

/// Decode conn_id + metadata + channels + payload for Response messages.
fn decode_response_payload(data: &[u8]) -> DecodedResponsePayloadWithConnId {
    if data.is_empty() {
        return Ok((
            roam_wire::ConnectionId::ROOT,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }
    let combined: CombinedPayload =
        facet_postcard::from_slice(data).map_err(|e| format!("decode error: {}", e))?;
    Ok((
        roam_wire::ConnectionId(combined.conn_id),
        combined.metadata,
        combined.channels,
        combined.payload,
    ))
}

/// Guest-side transport wrapper implementing `MessageTransport`.
///
/// This wraps an `ShmGuest` to provide the async interface expected by
/// roam's `Connection` type.
pub struct ShmGuestTransport {
    guest: ShmGuest,
    /// Buffer for last decoded bytes (for error detection)
    last_decoded: Vec<u8>,
    /// Doorbell for notifying the host of new messages
    doorbell: Option<shm_primitives::Doorbell>,
}

impl ShmGuestTransport {
    /// Create a new transport with doorbell signaling.
    pub fn new_with_doorbell(guest: ShmGuest, doorbell: shm_primitives::Doorbell) -> Self {
        Self {
            guest,
            last_decoded: Vec::new(),
            doorbell: Some(doorbell),
        }
    }

    /// Create a new transport from spawn args (includes doorbell setup).
    ///
    /// This is a convenience constructor that creates both the guest and doorbell
    /// from spawn args, which is the typical usage pattern.
    pub fn from_spawn_args(args: crate::spawn::SpawnArgs) -> io::Result<Self> {
        // Attach guest first (borrows args), then move doorbell handle
        let guest =
            ShmGuest::attach_with_ticket(&args).map_err(|e| io::Error::other(e.to_string()))?;
        let doorbell = shm_primitives::Doorbell::from_handle(args.doorbell_handle)?;
        Ok(Self::new_with_doorbell(guest, doorbell))
    }

    /// Get the underlying guest.
    pub fn guest(&self) -> &ShmGuest {
        &self.guest
    }

    /// Get a mutable reference to the underlying guest.
    pub fn guest_mut(&mut self) -> &mut ShmGuest {
        &mut self.guest
    }

    /// Get the segment configuration.
    ///
    /// Returns the config read from the segment header (max_payload_size,
    /// initial_credit, etc.).
    pub fn config(&self) -> &crate::layout::SegmentConfig {
        self.guest.config()
    }

    /// Send a message (async with backpressure).
    ///
    /// Encodes the message once as an ShmMsg, then retries with `&ShmMsg`
    /// (no clone) if slots are exhausted or the ring is full.
    pub async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let shm_msg =
            message_to_shm_msg(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        loop {
            match self.guest.send(&shm_msg) {
                Ok(()) => {
                    // Ring doorbell to notify host of new message
                    if let Some(doorbell) = &self.doorbell {
                        doorbell.signal().await;
                    }
                    return Ok(());
                }
                Err(SendError::SlotExhausted) => {
                    // Wait for host to free slots (it rings our doorbell when it does)
                    if let Some(doorbell) = &self.doorbell {
                        debug!("slot exhaustion: waiting for doorbell");
                        doorbell.wait().await?;
                        debug!("slot exhaustion: doorbell rang, retrying send");
                        continue;
                    } else {
                        return Err(io::Error::other("slot exhausted"));
                    }
                }
                Err(SendError::RingFull) => {
                    if let Some(doorbell) = &self.doorbell {
                        debug!("ring full: waiting for doorbell");
                        doorbell.wait().await?;
                        debug!("ring full: doorbell rang, retrying send");
                        continue;
                    } else {
                        return Err(io::Error::other("ring full"));
                    }
                }
                Err(SendError::HostGoodbye) => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionReset,
                        "host goodbye",
                    ));
                }
                Err(SendError::PayloadTooLarge) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "payload too large",
                    ));
                }
            }
        }
    }

    /// Try to receive a message (non-blocking).
    pub fn try_recv(&mut self) -> io::Result<Option<Message>> {
        match self.guest.recv() {
            Some(shm_msg) => {
                // Store raw bytes for error detection
                self.last_decoded = shm_msg.payload.clone();

                let msg = shm_msg_to_message(shm_msg)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Some(msg))
            }
            None => {
                if self.guest.is_host_goodbye() {
                    Ok(None)
                } else {
                    Err(io::Error::new(io::ErrorKind::WouldBlock, "no message"))
                }
            }
        }
    }

    /// Receive with timeout (blocking with spin/yield).
    pub fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
        let start = std::time::Instant::now();

        loop {
            match self.try_recv() {
                Ok(msg) => return Ok(msg),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    std::thread::yield_now();
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Receive (blocking until message arrives or connection closes).
    pub fn recv(&mut self) -> io::Result<Option<Message>> {
        loop {
            match self.try_recv() {
                Ok(msg) => return Ok(msg),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::yield_now();
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Get the last decoded bytes (for error detection).
    pub fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

/// Host-side transport for a single guest connection.
///
/// The host manages multiple guests, but each guest connection can be
/// wrapped in this transport to provide the `MessageTransport` interface.
pub struct ShmHostGuestTransport<'a> {
    host: &'a mut crate::host::ShmHost,
    peer_id: crate::peer::PeerId,
    /// Buffer for last decoded bytes
    last_decoded: Vec<u8>,
    /// Pending messages from poll
    pending: Vec<ShmMsg>,
}

impl<'a> ShmHostGuestTransport<'a> {
    /// Create a transport for a specific guest.
    pub fn new(host: &'a mut crate::host::ShmHost, peer_id: crate::peer::PeerId) -> Self {
        Self {
            host,
            peer_id,
            last_decoded: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Send a message to the guest.
    pub fn send(&mut self, msg: &Message) -> io::Result<()> {
        let shm_msg =
            message_to_shm_msg(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.host.send(self.peer_id, &shm_msg).map_err(|e| {
            use crate::host::SendError;
            match e {
                SendError::PeerNotAttached => {
                    io::Error::new(io::ErrorKind::NotConnected, "peer not attached")
                }
                SendError::RingFull => io::Error::other("ring full"),
                SendError::PayloadTooLarge => {
                    io::Error::new(io::ErrorKind::InvalidData, "payload too large")
                }
                SendError::SlotExhausted => io::Error::other("slot exhausted"),
            }
        })
    }

    /// Try to receive a message from this guest (non-blocking).
    pub fn try_recv(&mut self) -> io::Result<Option<Message>> {
        // Check pending first
        if let Some(shm_msg) = self.pending.pop() {
            self.last_decoded = shm_msg.payload.clone();
            let msg = shm_msg_to_message(shm_msg)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            return Ok(Some(msg));
        }

        // Poll for new messages from all guests
        // Note: slots_freed_for is ignored here - this transport doesn't have doorbell access
        // (the MultiPeerHostDriver handles backpressure signaling properly)
        let result = self.host.poll();
        for (peer_id, shm_msg) in result.messages {
            if peer_id == self.peer_id {
                self.last_decoded = shm_msg.payload.clone();
                let msg = shm_msg_to_message(shm_msg)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                return Ok(Some(msg));
            }
        }

        Err(io::Error::new(io::ErrorKind::WouldBlock, "no message"))
    }

    /// Get the last decoded bytes.
    pub fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

// Implement MessageTransport for ShmGuestTransport
mod async_transport {
    use super::*;
    use roam_stream::MessageTransport;
    use std::time::Duration;

    impl MessageTransport for ShmGuestTransport {
        async fn send(&mut self, msg: &Message) -> io::Result<()> {
            ShmGuestTransport::send(self, msg).await
        }

        async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
            async fn signal_and_return(
                doorbell: &Option<shm_primitives::Doorbell>,
                msg: Option<Message>,
            ) -> io::Result<Option<Message>> {
                if msg.is_some() {
                    // shm[impl shm.backpressure.host-to-guest]
                    if let Some(doorbell) = doorbell {
                        doorbell.signal().await;
                    }
                }
                Ok(msg)
            }

            // First check if there's already a message waiting
            match self.try_recv() {
                Ok(msg) => return signal_and_return(&self.doorbell, msg).await,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }

            // Wait on doorbell with timeout
            if let Some(doorbell) = &self.doorbell {
                match tokio::time::timeout(timeout, doorbell.wait()).await {
                    Ok(Ok(())) => match self.try_recv() {
                        Ok(msg) => signal_and_return(&self.doorbell, msg).await,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                        Err(e) => Err(e),
                    },
                    Ok(Err(e)) => Err(e),
                    Err(_timeout) => Ok(None),
                }
            } else {
                tokio::task::yield_now().await;
                match self.try_recv() {
                    Ok(msg) => signal_and_return(&self.doorbell, msg).await,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }

        async fn recv(&mut self) -> io::Result<Option<Message>> {
            loop {
                match self.try_recv() {
                    Ok(msg) => {
                        // shm[impl shm.backpressure.host-to-guest]
                        if let Some(doorbell) = &self.doorbell {
                            doorbell.signal().await;
                        }
                        return Ok(msg);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e),
                }

                if let Some(doorbell) = &self.doorbell {
                    doorbell.wait().await?;
                } else {
                    tokio::task::yield_now().await;
                }
            }
        }

        fn last_decoded(&self) -> &[u8] {
            ShmGuestTransport::last_decoded(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roam_wire::{ConnectionId, Hello, MetadataValue};

    #[test]
    fn roundtrip_request() {
        let msg = Message::Request {
            conn_id: ConnectionId::ROOT,
            request_id: 42,
            method_id: 123,
            metadata: vec![(
                "key".to_string(),
                MetadataValue::String("value".to_string()),
                0,
            )],
            channels: vec![],
            payload: b"hello world".to_vec(),
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        let decoded = shm_msg_to_message(shm_msg).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_request_with_channels() {
        let msg = Message::Request {
            conn_id: ConnectionId::ROOT,
            request_id: 42,
            method_id: 123,
            metadata: vec![],
            channels: vec![1, 3, 5],
            payload: b"hello world".to_vec(),
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        let decoded = shm_msg_to_message(shm_msg).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_response() {
        let msg = Message::Response {
            conn_id: ConnectionId::ROOT,
            request_id: 99,
            metadata: vec![],
            channels: vec![],
            payload: b"response data".to_vec(),
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        let decoded = shm_msg_to_message(shm_msg).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_response_with_channels() {
        let msg = Message::Response {
            conn_id: ConnectionId::ROOT,
            request_id: 99,
            metadata: vec![],
            channels: vec![2, 4, 6],
            payload: b"response data".to_vec(),
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        let decoded = shm_msg_to_message(shm_msg).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_data() {
        let msg = Message::Data {
            conn_id: ConnectionId::ROOT,
            channel_id: 7,
            payload: b"stream chunk".to_vec(),
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        let decoded = shm_msg_to_message(shm_msg).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_control_messages() {
        let messages = vec![
            Message::Cancel {
                conn_id: ConnectionId::ROOT,
                request_id: 10,
            },
            Message::Close {
                conn_id: ConnectionId::ROOT,
                channel_id: 20,
            },
            Message::Reset {
                conn_id: ConnectionId::ROOT,
                channel_id: 30,
            },
            Message::Goodbye {
                conn_id: ConnectionId::ROOT,
                reason: "shutdown".to_string(),
            },
        ];

        for msg in messages {
            let shm_msg = message_to_shm_msg(&msg).unwrap();
            let decoded = shm_msg_to_message(shm_msg).unwrap();
            assert_eq!(msg, decoded);
        }
    }

    #[test]
    fn hello_not_supported() {
        let msg = Message::Hello(Hello::V4 {
            max_payload_size: 64 * 1024,
            initial_channel_credit: 64 * 1024,
        });

        assert!(matches!(
            message_to_shm_msg(&msg),
            Err(ConvertError::HelloNotSupported)
        ));
    }

    #[test]
    fn credit_not_supported() {
        let msg = Message::Credit {
            conn_id: ConnectionId::ROOT,
            channel_id: 1,
            bytes: 1024,
        };

        assert!(matches!(
            message_to_shm_msg(&msg),
            Err(ConvertError::CreditNotSupported)
        ));
    }

    #[test]
    fn small_payload_roundtrip() {
        let msg = Message::Data {
            conn_id: ConnectionId::ROOT,
            channel_id: 1,
            payload: b"tiny".to_vec(),
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        // Payload is conn_id (8 bytes) + "tiny" (4 bytes) = 12 bytes
        assert_eq!(shm_msg.payload.len(), 12);
        let decoded = shm_msg_to_message(shm_msg).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn large_payload_roundtrip() {
        let msg = Message::Data {
            conn_id: ConnectionId::ROOT,
            channel_id: 1,
            payload: vec![0u8; 100],
        };

        let shm_msg = message_to_shm_msg(&msg).unwrap();
        assert_eq!(shm_msg.payload.len(), 108); // 8 bytes conn_id + 100
        let decoded = shm_msg_to_message(shm_msg).unwrap();
        assert_eq!(msg, decoded);
    }
}
