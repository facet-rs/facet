//! Handle + Driver transport abstraction.
//!
//! This module provides the new transport abstraction where:
//! - `TransportHandle` is the public API for sending/receiving frames
//! - `TransportDriver` is a future that owns the I/O and runs the mux/demux loop
//!
//! The handle is cheap to clone and can be shared across tasks.
//! The driver must be spawned (or polled) by the caller.

use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;

use crate::TransportError;

/// A handle for sending and receiving frames.
///
/// This is the public API for transport I/O. The handle is cheap to clone
/// and can be shared across tasks. Internally it communicates with a
/// driver task that owns the actual I/O object.
///
/// Payload types are transport-specific:
/// - Stream/WebSocket: pooled buffers or `Bytes`
/// - SHM: slot guards
pub trait TransportHandle: Send + Sync + Clone + 'static {
    /// Payload type for sending (must be 'static, owned).
    type SendPayload: Deref<Target = [u8]> + Send + 'static;

    /// Payload type for receiving.
    type RecvPayload: Deref<Target = [u8]> + Send + 'static;

    /// Send a frame. Awaits if backpressure is applied.
    fn send_frame(
        &self,
        frame: SendFrame<Self::SendPayload>,
    ) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Receive a frame.
    fn recv_frame(
        &self,
    ) -> impl Future<Output = Result<RecvFrame<Self::RecvPayload>, TransportError>> + Send;

    /// Signal close. Non-blocking, initiates graceful shutdown.
    fn close(&self);

    /// Check if the transport is closed or failed.
    fn is_closed(&self) -> bool;
}

/// Frame for sending with generic payload.
#[derive(Debug, Clone)]
pub struct SendFrame<P> {
    /// The frame descriptor.
    pub desc: crate::MsgDescHot,
    /// Optional payload (None if inline or empty).
    pub payload: Option<P>,
}

impl<P: Deref<Target = [u8]>> SendFrame<P> {
    /// Create a new send frame with the given descriptor.
    pub fn new(desc: crate::MsgDescHot) -> Self {
        Self {
            desc,
            payload: None,
        }
    }

    /// Create a frame with external payload.
    pub fn with_payload(mut desc: crate::MsgDescHot, payload: P) -> Self {
        desc.payload_slot = 0;
        desc.payload_len = payload.deref().len() as u32;
        Self {
            desc,
            payload: Some(payload),
        }
    }

    /// Get the payload bytes.
    pub fn payload_bytes(&self) -> &[u8] {
        if self.desc.is_inline() {
            self.desc.inline_payload()
        } else if let Some(ref p) = self.payload {
            p.deref()
        } else {
            &[]
        }
    }
}

impl SendFrame<Vec<u8>> {
    /// Create a frame with inline payload.
    pub fn with_inline_payload(mut desc: crate::MsgDescHot, payload: &[u8]) -> Option<Self> {
        if payload.len() > crate::INLINE_PAYLOAD_SIZE {
            return None;
        }
        desc.payload_slot = crate::INLINE_PAYLOAD_SLOT;
        desc.payload_generation = 0;
        desc.payload_offset = 0;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(payload);
        Some(Self {
            desc,
            payload: None,
        })
    }
}

/// Received frame with owned descriptor and a payload handle.
///
/// Re-exported from frame.rs for convenience.
pub use crate::RecvFrame;

/// The driver future type.
///
/// This is a boxed future that owns the I/O object and runs the mux/demux loop.
/// The caller must spawn or poll this future.
///
/// For SHM transport, this can be a no-op future if no background work is needed.
pub type TransportDriver =
    Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'static>>;

/// Result of splitting a transport into handle + driver.
pub struct TransportParts<H> {
    /// The handle for sending/receiving frames.
    pub handle: H,
    /// The driver future to spawn.
    pub driver: TransportDriver,
}

/// Trait for types that can be split into handle + driver.
pub trait IntoTransportParts {
    /// The handle type.
    type Handle: TransportHandle;

    /// Split into handle + driver.
    fn into_parts(self) -> TransportParts<Self::Handle>;
}

// QoS types for future use

/// Quality of service class for prioritization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QoSClass {
    /// Control frames (ping, close, cancel) - highest priority.
    Control,
    /// High priority data.
    High,
    /// Normal priority (default).
    #[default]
    Normal,
    /// Bulk/background data - lowest priority.
    Bulk,
}

/// Message wrapper with QoS metadata.
#[derive(Debug, Clone)]
pub struct Prioritized<T> {
    /// The QoS class.
    pub class: QoSClass,
    /// The wrapped item.
    pub item: T,
}

impl<T> Prioritized<T> {
    /// Create a new prioritized item with Normal class.
    pub fn normal(item: T) -> Self {
        Self {
            class: QoSClass::Normal,
            item,
        }
    }

    /// Create a new prioritized item with the given class.
    pub fn with_class(class: QoSClass, item: T) -> Self {
        Self { class, item }
    }
}
