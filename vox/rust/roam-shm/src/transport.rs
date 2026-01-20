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

/// Decoded metadata, channels, and payload from a Response message.
type DecodedResponsePayload = Result<(Vec<(String, MetadataValue)>, Vec<u64>, Vec<u8>), String>;

/// Decoded metadata, channels, and payload from a Request message.
type DecodedRequestPayload = Result<(Vec<(String, MetadataValue)>, Vec<u64>, Vec<u8>), String>;

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
            channels,
            payload,
        } => {
            // shm[impl shm.metadata.in-payload]
            // Encode metadata + channels + payload together
            let combined = encode_request_payload(metadata, channels, payload);

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
            channels,
            payload,
        } => {
            // shm[impl shm.metadata.in-payload]
            let combined = encode_response_payload(metadata, channels, payload);

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
            let (metadata, channels, payload) = decode_request_payload(payload_bytes)
                .map_err(|e| ConvertError::DecodeError(e.to_string()))?;

            Ok(Message::Request {
                request_id: frame.desc.id as u64,
                method_id: frame.desc.method_id,
                metadata,
                channels,
                payload,
            })
        }

        msg_type::RESPONSE => {
            let (metadata, channels, payload) = decode_response_payload(payload_bytes)
                .map_err(|e| ConvertError::DecodeError(e.to_string()))?;

            Ok(Message::Response {
                request_id: frame.desc.id as u64,
                metadata,
                channels,
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

/// Combined payload for Request/Response messages.
#[derive(facet::Facet)]
struct CombinedPayload {
    metadata: Vec<(String, MetadataValue)>,
    channels: Vec<u64>,
    payload: Vec<u8>,
}

/// Encode metadata + channels + payload for Request messages.
fn encode_request_payload(
    metadata: &[(String, MetadataValue)],
    channels: &[u64],
    payload: &[u8],
) -> Vec<u8> {
    let combined = CombinedPayload {
        metadata: metadata.to_vec(),
        channels: channels.to_vec(),
        payload: payload.to_vec(),
    };
    let result = facet_postcard::to_vec(&combined).unwrap_or_default();
    tracing::debug!(
        channels = ?channels,
        result_len = result.len(),
        "encode_request_payload"
    );
    result
}

/// Encode metadata + channels + payload for Response messages.
fn encode_response_payload(
    metadata: &[(String, MetadataValue)],
    channels: &[u64],
    payload: &[u8],
) -> Vec<u8> {
    let combined = CombinedPayload {
        metadata: metadata.to_vec(),
        channels: channels.to_vec(),
        payload: payload.to_vec(),
    };
    facet_postcard::to_vec(&combined).unwrap_or_default()
}

/// Decode metadata + channels + payload for Request messages.
fn decode_request_payload(data: &[u8]) -> DecodedRequestPayload {
    tracing::debug!(
        data_len = data.len(),
        "decode_request_payload: input"
    );
    if data.is_empty() {
        tracing::debug!("decode_request_payload: empty data, returning empty");
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let combined: CombinedPayload = facet_postcard::from_slice(data)
        .map_err(|e| format!("decode error: {}", e))?;
    tracing::debug!(
        channels = ?combined.channels,
        "decode_request_payload: decoded"
    );
    Ok((combined.metadata, combined.channels, combined.payload))
}

/// Decode metadata + channels + payload for Response messages.
fn decode_response_payload(data: &[u8]) -> DecodedResponsePayload {
    if data.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let combined: CombinedPayload = facet_postcard::from_slice(data)
        .map_err(|e| format!("decode error: {}", e))?;
    Ok((combined.metadata, combined.channels, combined.payload))
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
    /// If slots are exhausted, waits for the doorbell (host signals when slots are freed)
    /// and retries. This provides backpressure instead of failing immediately.
    pub async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let frame =
            message_to_frame(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        loop {
            match self.guest.send(frame.clone()) {
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
                        // Retry after wakeup
                        continue;
                    } else {
                        // No doorbell - can't wait, must fail
                        return Err(io::Error::other("slot exhausted"));
                    }
                }
                Err(SendError::RingFull) => {
                    // Ring full - also wait for doorbell and retry
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
        if let Some(frame) = self.pending.pop() {
            self.last_decoded = frame.payload_bytes().to_vec();
            let msg = frame_to_message(frame)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            return Ok(Some(msg));
        }

        // Poll for new messages from all guests
        // Note: slots_freed_for is ignored here - this transport doesn't have doorbell access
        // (the MultiPeerHostDriver handles backpressure signaling properly)
        let result = self.host.poll();
        for (peer_id, frame) in result.messages {
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

// Implement MessageTransport for ShmGuestTransport
mod async_transport {
    use super::*;
    use roam_stream::MessageTransport;
    use std::time::Duration;

    impl MessageTransport for ShmGuestTransport {
        /// Send a message over the SHM transport.
        ///
        /// If slots are exhausted, waits for the doorbell (host signals when slots are freed)
        /// and retries. This provides backpressure instead of failing immediately.
        async fn send(&mut self, msg: &Message) -> io::Result<()> {
            ShmGuestTransport::send(self, msg).await
        }

        /// Receive a message with timeout.
        ///
        /// Waits on doorbell for host notifications, with a timeout.
        async fn recv_timeout(&mut self, timeout: Duration) -> io::Result<Option<Message>> {
            // Helper to signal doorbell after receiving
            async fn signal_and_return(
                doorbell: &Option<shm_primitives::Doorbell>,
                msg: Option<Message>,
            ) -> io::Result<Option<Message>> {
                if msg.is_some() {
                    // Signal doorbell to notify host that we consumed a message
                    // (host may have pending sends waiting for slots to free up)
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
                    Ok(Ok(())) => {
                        // Doorbell rang, try to receive
                        match self.try_recv() {
                            Ok(msg) => signal_and_return(&self.doorbell, msg).await,
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                            Err(e) => Err(e),
                        }
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_timeout) => Ok(None),
                }
            } else {
                // No doorbell - fall back to yielding (shouldn't happen in practice)
                tokio::task::yield_now().await;
                match self.try_recv() {
                    Ok(msg) => signal_and_return(&self.doorbell, msg).await,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }

        /// Receive a message (waits on doorbell until one arrives or connection closes).
        async fn recv(&mut self) -> io::Result<Option<Message>> {
            loop {
                // First check if there's already a message waiting
                match self.try_recv() {
                    Ok(msg) => {
                        // Signal doorbell to notify host that we consumed a message
                        // (host may have pending sends waiting for slots to free up)
                        // shm[impl shm.backpressure.host-to-guest]
                        if let Some(doorbell) = &self.doorbell {
                            doorbell.signal().await;
                        }
                        return Ok(msg);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e),
                }

                // Wait on doorbell for host notification
                if let Some(doorbell) = &self.doorbell {
                    doorbell.wait().await?;
                } else {
                    // No doorbell - yield and retry (shouldn't happen in practice)
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
            channels: vec![],
            payload: b"hello world".to_vec(),
        };

        let frame = message_to_frame(&msg).unwrap();
        let decoded = frame_to_message(frame).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_request_with_channels() {
        let msg = Message::Request {
            request_id: 42,
            method_id: 123,
            metadata: vec![],
            channels: vec![1, 3, 5],
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
            channels: vec![],
            payload: b"response data".to_vec(),
        };

        let frame = message_to_frame(&msg).unwrap();
        let decoded = frame_to_message(frame).unwrap();

        assert_eq!(msg, decoded);
    }

    #[test]
    fn roundtrip_response_with_channels() {
        let msg = Message::Response {
            request_id: 99,
            metadata: vec![],
            channels: vec![2, 4, 6],
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
