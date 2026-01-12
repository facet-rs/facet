//! Transport abstraction for SHM.
//!
//! This module provides the bridge between roam's `MessageTransport` trait
//! and the SHM Frame-based communication. It handles:
//!
//! - Converting between `roam_wire::Message` and SHM `Frame`
//! - Encoding/decoding metadata alongside payload
//! - Async wrappers for the synchronous SHM operations
//!
//! shm[impl shm.scope]

use std::io;
use std::time::Duration;

use roam_frame::{Frame, INLINE_PAYLOAD_LEN, INLINE_PAYLOAD_SLOT, MsgDesc, Payload};
use roam_wire::{Message, MetadataValue};

use crate::guest::{SendError, ShmGuest};
use crate::msg::msg_type;

/// Decoded metadata and payload from a message.
type DecodedPayload = Result<(Vec<(String, MetadataValue)>, Vec<u8>), String>;

/// Conversion error when mapping between Message and Frame.
#[derive(Debug)]
pub enum ConvertError {
    /// Unknown message type in frame
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

/// Convert a `roam_wire::Message` to an SHM `Frame`.
///
/// shm[impl shm.metadata.in-payload]
///
/// For Request and Response, metadata is prepended to the payload and
/// encoded together using postcard.
pub fn message_to_frame(msg: &Message) -> Result<Frame, ConvertError> {
    match msg {
        Message::Hello(_) => {
            // shm[impl shm.handshake]
            // SHM doesn't use Hello messages - handshake is implicit via segment header
            Err(ConvertError::HelloNotSupported)
        }

        Message::Goodbye { reason } => {
            // Goodbye uses the payload for the reason string
            let mut desc = MsgDesc::new(msg_type::GOODBYE, 0, 0);
            let reason_bytes = reason.as_bytes();

            let payload = if reason_bytes.len() <= INLINE_PAYLOAD_LEN {
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_len = reason_bytes.len() as u32;
                desc.inline_payload[..reason_bytes.len()].copy_from_slice(reason_bytes);
                Payload::Inline
            } else {
                Payload::Owned(reason_bytes.to_vec())
            };

            Ok(Frame { desc, payload })
        }

        Message::Request {
            request_id,
            method_id,
            metadata,
            payload,
        } => {
            // shm[impl shm.metadata.in-payload]
            // Encode metadata + payload together
            let combined = encode_request_payload(metadata, payload);

            let mut desc = MsgDesc::new(msg_type::REQUEST, *request_id as u32, *method_id);

            let frame_payload = if combined.len() <= INLINE_PAYLOAD_LEN {
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_len = combined.len() as u32;
                desc.inline_payload[..combined.len()].copy_from_slice(&combined);
                Payload::Inline
            } else {
                desc.payload_len = combined.len() as u32;
                Payload::Owned(combined)
            };

            Ok(Frame {
                desc,
                payload: frame_payload,
            })
        }

        Message::Response {
            request_id,
            metadata,
            payload,
        } => {
            // shm[impl shm.metadata.in-payload]
            let combined = encode_response_payload(metadata, payload);

            let mut desc = MsgDesc::new(msg_type::RESPONSE, *request_id as u32, 0);

            let frame_payload = if combined.len() <= INLINE_PAYLOAD_LEN {
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_len = combined.len() as u32;
                desc.inline_payload[..combined.len()].copy_from_slice(&combined);
                Payload::Inline
            } else {
                desc.payload_len = combined.len() as u32;
                Payload::Owned(combined)
            };

            Ok(Frame {
                desc,
                payload: frame_payload,
            })
        }

        Message::Cancel { request_id } => {
            let desc = MsgDesc::new(msg_type::CANCEL, *request_id as u32, 0);
            Ok(Frame {
                desc,
                payload: Payload::Inline,
            })
        }

        Message::Data {
            channel_id,
            payload,
        } => {
            let mut desc = MsgDesc::new(msg_type::DATA, *channel_id as u32, 0);

            let frame_payload = if payload.len() <= INLINE_PAYLOAD_LEN {
                desc.payload_slot = INLINE_PAYLOAD_SLOT;
                desc.payload_len = payload.len() as u32;
                desc.inline_payload[..payload.len()].copy_from_slice(payload);
                Payload::Inline
            } else {
                desc.payload_len = payload.len() as u32;
                Payload::Owned(payload.clone())
            };

            Ok(Frame {
                desc,
                payload: frame_payload,
            })
        }

        Message::Close { channel_id } => {
            let desc = MsgDesc::new(msg_type::CLOSE, *channel_id as u32, 0);
            Ok(Frame {
                desc,
                payload: Payload::Inline,
            })
        }

        Message::Reset { channel_id } => {
            let desc = MsgDesc::new(msg_type::RESET, *channel_id as u32, 0);
            Ok(Frame {
                desc,
                payload: Payload::Inline,
            })
        }

        Message::Credit { .. } => {
            // shm[impl shm.flow.no-credit-message]
            // SHM uses channel table for flow control, not Credit messages
            Err(ConvertError::CreditNotSupported)
        }
    }
}

/// Convert an SHM `Frame` to a `roam_wire::Message`.
///
/// shm[impl shm.metadata.in-payload]
pub fn frame_to_message(frame: Frame) -> Result<Message, ConvertError> {
    let payload_bytes = frame.payload_bytes();

    match frame.desc.msg_type {
        msg_type::GOODBYE => {
            let reason = String::from_utf8_lossy(payload_bytes).into_owned();
            Ok(Message::Goodbye { reason })
        }

        msg_type::REQUEST => {
            let (metadata, payload) = decode_request_payload(payload_bytes)
                .map_err(|e| ConvertError::DecodeError(e.to_string()))?;

            Ok(Message::Request {
                request_id: frame.desc.id as u64,
                method_id: frame.desc.method_id,
                metadata,
                payload,
            })
        }

        msg_type::RESPONSE => {
            let (metadata, payload) = decode_response_payload(payload_bytes)
                .map_err(|e| ConvertError::DecodeError(e.to_string()))?;

            Ok(Message::Response {
                request_id: frame.desc.id as u64,
                metadata,
                payload,
            })
        }

        msg_type::CANCEL => Ok(Message::Cancel {
            request_id: frame.desc.id as u64,
        }),

        msg_type::DATA => Ok(Message::Data {
            channel_id: frame.desc.id as u64,
            payload: payload_bytes.to_vec(),
        }),

        msg_type::CLOSE => Ok(Message::Close {
            channel_id: frame.desc.id as u64,
        }),

        msg_type::RESET => Ok(Message::Reset {
            channel_id: frame.desc.id as u64,
        }),

        other => Err(ConvertError::UnknownMsgType(other)),
    }
}

/// Encode metadata + payload for Request messages.
///
/// Format: [metadata_len: varint][metadata: postcard][payload: raw bytes]
fn encode_request_payload(metadata: &[(String, MetadataValue)], payload: &[u8]) -> Vec<u8> {
    // Encode metadata with postcard
    let metadata_vec: Vec<(String, MetadataValue)> = metadata.to_vec();
    let metadata_bytes = facet_postcard::to_vec(&metadata_vec).unwrap_or_default();

    // Build combined payload: metadata_len (varint) + metadata + payload
    let mut combined = Vec::with_capacity(5 + metadata_bytes.len() + payload.len());

    // Write metadata length as varint
    let mut len = metadata_bytes.len();
    while len >= 0x80 {
        combined.push((len as u8) | 0x80);
        len >>= 7;
    }
    combined.push(len as u8);

    combined.extend_from_slice(&metadata_bytes);
    combined.extend_from_slice(payload);

    combined
}

/// Encode metadata + payload for Response messages.
fn encode_response_payload(metadata: &[(String, MetadataValue)], payload: &[u8]) -> Vec<u8> {
    // Same format as request
    encode_request_payload(metadata, payload)
}

/// Decode metadata + payload for Request messages.
fn decode_request_payload(data: &[u8]) -> DecodedPayload {
    if data.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Read metadata length as varint
    let mut pos = 0;
    let mut metadata_len: usize = 0;
    let mut shift = 0;

    loop {
        if pos >= data.len() {
            return Err("truncated varint".to_string());
        }
        let byte = data[pos];
        pos += 1;
        metadata_len |= ((byte & 0x7F) as usize) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift > 28 {
            return Err("varint too large".to_string());
        }
    }

    if pos + metadata_len > data.len() {
        return Err("metadata extends past end of data".to_string());
    }

    let metadata_bytes = &data[pos..pos + metadata_len];
    let payload = data[pos + metadata_len..].to_vec();

    let metadata: Vec<(String, MetadataValue)> = if metadata_len == 0 {
        Vec::new()
    } else {
        facet_postcard::from_slice(metadata_bytes)
            .map_err(|e| format!("metadata decode error: {}", e))?
    };

    Ok((metadata, payload))
}

/// Decode metadata + payload for Response messages.
fn decode_response_payload(data: &[u8]) -> DecodedPayload {
    // Same format as request
    decode_request_payload(data)
}

/// Guest-side transport wrapper implementing `MessageTransport`.
///
/// This wraps an `ShmGuest` to provide the async interface expected by
/// roam's `Connection` type.
pub struct ShmGuestTransport {
    guest: ShmGuest,
    /// Buffer for last decoded bytes (for error detection)
    last_decoded: Vec<u8>,
}

impl ShmGuestTransport {
    /// Create a new transport wrapper around an ShmGuest.
    pub fn new(guest: ShmGuest) -> Self {
        Self {
            guest,
            last_decoded: Vec::new(),
        }
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

    /// Send a message.
    pub fn send(&mut self, msg: &Message) -> io::Result<()> {
        let frame =
            message_to_frame(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.guest.send(frame).map_err(|e| match e {
            SendError::HostGoodbye => {
                io::Error::new(io::ErrorKind::ConnectionReset, "host goodbye")
            }
            SendError::RingFull => io::Error::new(io::ErrorKind::WouldBlock, "ring full"),
            SendError::PayloadTooLarge => {
                io::Error::new(io::ErrorKind::InvalidData, "payload too large")
            }
            SendError::SlotExhausted => io::Error::new(io::ErrorKind::WouldBlock, "slot exhausted"),
        })
    }

    /// Try to receive a message (non-blocking).
    pub fn try_recv(&mut self) -> io::Result<Option<Message>> {
        match self.guest.recv() {
            Some(frame) => {
                // Store raw bytes for error detection
                self.last_decoded = frame.payload_bytes().to_vec();

                let msg = frame_to_message(frame)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Some(msg))
            }
            None => {
                if self.guest.is_host_goodbye() {
                    // Connection closed
                    Ok(None)
                } else {
                    // No message available
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
                    // Yield to avoid busy-spinning
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
    pending: Vec<Frame>,
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
        let frame =
            message_to_frame(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.host.send(self.peer_id, frame).map_err(|e| {
            use crate::host::SendError;
            match e {
                SendError::PeerNotAttached => {
                    io::Error::new(io::ErrorKind::NotConnected, "peer not attached")
                }
                SendError::RingFull => io::Error::new(io::ErrorKind::WouldBlock, "ring full"),
                SendError::PayloadTooLarge => {
                    io::Error::new(io::ErrorKind::InvalidData, "payload too large")
                }
                SendError::SlotExhausted => {
                    io::Error::new(io::ErrorKind::WouldBlock, "slot exhausted")
                }
            }
        })
    }

    /// Try to receive a message from this guest (non-blocking).
    pub fn try_recv(&mut self) -> io::Result<Option<Message>> {
        // Check pending first
        if let Some(frame) = self.pending.pop() {
            self.last_decoded = frame.payload_bytes().to_vec();
            let msg = frame_to_message(frame)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            return Ok(Some(msg));
        }

        // Poll for new messages from all guests
        let messages = self.host.poll();
        for (peer_id, frame) in messages {
            if peer_id == self.peer_id {
                self.last_decoded = frame.payload_bytes().to_vec();
                let msg = frame_to_message(frame)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                return Ok(Some(msg));
            } else {
                // Store for other transports (would need shared state for this)
                // For now, we just process our own messages
            }
        }

        Err(io::Error::new(io::ErrorKind::WouldBlock, "no message"))
    }

    /// Get the last decoded bytes.
    pub fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

// Implement MessageTransport for ShmGuestTransport when tokio feature is enabled
#[cfg(feature = "tokio")]
mod async_transport {
    use super::*;
    use roam_stream::MessageTransport;
    use std::time::Duration;

    impl MessageTransport for ShmGuestTransport {
        /// Send a message over the SHM transport.
        ///
        /// This is synchronous internally but wrapped as async for the trait.
        async fn send(&mut self, msg: &Message) -> io::Result<()> {
            // SHM send is synchronous and fast, no need for spawn_blocking
            ShmGuestTransport::send(self, msg)
        }

        /// Receive a message with timeout.
        ///
        /// Uses tokio's timeout and yields between poll attempts.
        async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
            let deadline = tokio::time::Instant::now() + timeout;

            loop {
                match self.try_recv() {
                    Ok(msg) => return Ok(msg),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        if tokio::time::Instant::now() >= deadline {
                            return Ok(None);
                        }
                        // Yield to the async runtime
                        tokio::task::yield_now().await;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        /// Receive a message (async, yields between poll attempts).
        async fn recv(&mut self) -> io::Result<Option<Message>> {
            loop {
                match self.try_recv() {
                    Ok(msg) => return Ok(msg),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Yield to the async runtime to avoid blocking
                        tokio::task::yield_now().await;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        fn last_decoded(&self) -> &[u8] {
            ShmGuestTransport::last_decoded(self)
        }
    }
}

/// Owned host-side transport for a single guest connection.
///
/// Unlike `ShmHostGuestTransport` which borrows the host, this transport
/// owns the host. This allows it to be used with async drivers that need
/// to own their transport.
///
/// Note: This is intended for single-guest scenarios or when the host
/// is dedicated to one peer connection.
pub struct OwnedShmHostTransport {
    host: crate::host::ShmHost,
    peer_id: crate::peer::PeerId,
    /// Buffer for last decoded bytes
    last_decoded: Vec<u8>,
}

impl OwnedShmHostTransport {
    /// Create a transport that owns the host for a specific peer.
    pub fn new(host: crate::host::ShmHost, peer_id: crate::peer::PeerId) -> Self {
        Self {
            host,
            peer_id,
            last_decoded: Vec::new(),
        }
    }

    /// Get the peer ID this transport communicates with.
    pub fn peer_id(&self) -> crate::peer::PeerId {
        self.peer_id
    }

    /// Get a reference to the underlying host.
    pub fn host(&self) -> &crate::host::ShmHost {
        &self.host
    }

    /// Get a mutable reference to the underlying host.
    pub fn host_mut(&mut self) -> &mut crate::host::ShmHost {
        &mut self.host
    }

    /// Get the segment configuration.
    pub fn config(&self) -> &crate::layout::SegmentConfig {
        self.host.config()
    }

    /// Send a message to the guest.
    pub fn send(&mut self, msg: &Message) -> io::Result<()> {
        let frame =
            message_to_frame(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.host.send(self.peer_id, frame).map_err(|e| {
            use crate::host::SendError;
            match e {
                SendError::PeerNotAttached => {
                    io::Error::new(io::ErrorKind::NotConnected, "peer not attached")
                }
                SendError::RingFull => io::Error::new(io::ErrorKind::WouldBlock, "ring full"),
                SendError::PayloadTooLarge => {
                    io::Error::new(io::ErrorKind::InvalidData, "payload too large")
                }
                SendError::SlotExhausted => {
                    io::Error::new(io::ErrorKind::WouldBlock, "slot exhausted")
                }
            }
        })
    }

    /// Try to receive a message from the peer (non-blocking).
    pub fn try_recv(&mut self) -> io::Result<Option<Message>> {
        // Poll for new messages from all guests
        let messages = self.host.poll();
        for (peer_id, frame) in messages {
            if peer_id == self.peer_id {
                self.last_decoded = frame.payload_bytes().to_vec();
                let msg = frame_to_message(frame)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                return Ok(Some(msg));
            }
            // Messages from other peers are dropped - this transport is single-peer
        }

        Err(io::Error::new(io::ErrorKind::WouldBlock, "no message"))
    }

    /// Get the last decoded bytes.
    pub fn last_decoded(&self) -> &[u8] {
        &self.last_decoded
    }
}

#[cfg(feature = "tokio")]
impl roam_stream::MessageTransport for OwnedShmHostTransport {
    async fn send(&mut self, msg: &Message) -> io::Result<()> {
        OwnedShmHostTransport::send(self, msg)
    }

    async fn recv_timeout(&mut self, timeout: std::time::Duration) -> io::Result<Option<Message>> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            match self.try_recv() {
                Ok(msg) => return Ok(msg),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if tokio::time::Instant::now() >= deadline {
                        return Ok(None);
                    }
                    tokio::task::yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn recv(&mut self) -> io::Result<Option<Message>> {
        loop {
            match self.try_recv() {
                Ok(msg) => return Ok(msg),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    tokio::task::yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn last_decoded(&self) -> &[u8] {
        OwnedShmHostTransport::last_decoded(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roam_wire::Hello;

    #[test]
    fn roundtrip_request() {
        let msg = Message::Request {
            request_id: 42,
            method_id: 123,
            metadata: vec![(
                "key".to_string(),
                MetadataValue::String("value".to_string()),
            )],
            payload: b"hello world".to_vec(),
        };

        let frame = message_to_frame(&msg).unwrap();
        let decoded = frame_to_message(frame).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_response() {
        let msg = Message::Response {
            request_id: 99,
            metadata: vec![],
            payload: b"response data".to_vec(),
        };

        let frame = message_to_frame(&msg).unwrap();
        let decoded = frame_to_message(frame).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_data() {
        let msg = Message::Data {
            channel_id: 7,
            payload: b"stream chunk".to_vec(),
        };

        let frame = message_to_frame(&msg).unwrap();
        let decoded = frame_to_message(frame).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_control_messages() {
        let messages = vec![
            Message::Cancel { request_id: 10 },
            Message::Close { channel_id: 20 },
            Message::Reset { channel_id: 30 },
            Message::Goodbye {
                reason: "shutdown".to_string(),
            },
        ];

        for msg in messages {
            let frame = message_to_frame(&msg).unwrap();
            let decoded = frame_to_message(frame).unwrap();
            assert_eq!(msg, decoded);
        }
    }

    #[test]
    fn hello_not_supported() {
        let msg = Message::Hello(Hello::V1 {
            max_payload_size: 64 * 1024,
            initial_channel_credit: 64 * 1024,
        });

        assert!(matches!(
            message_to_frame(&msg),
            Err(ConvertError::HelloNotSupported)
        ));
    }

    #[test]
    fn credit_not_supported() {
        let msg = Message::Credit {
            channel_id: 1,
            bytes: 1024,
        };

        assert!(matches!(
            message_to_frame(&msg),
            Err(ConvertError::CreditNotSupported)
        ));
    }

    #[test]
    fn inline_payload() {
        // Small payload should be inlined
        let msg = Message::Data {
            channel_id: 1,
            payload: b"tiny".to_vec(),
        };

        let frame = message_to_frame(&msg).unwrap();
        assert_eq!(frame.desc.payload_slot, INLINE_PAYLOAD_SLOT);
        assert!(matches!(frame.payload, Payload::Inline));
    }

    #[test]
    fn large_payload() {
        // Large payload should not be inlined
        let msg = Message::Data {
            channel_id: 1,
            payload: vec![0u8; 100],
        };

        let frame = message_to_frame(&msg).unwrap();
        assert!(matches!(frame.payload, Payload::Owned(_)));
    }
}
