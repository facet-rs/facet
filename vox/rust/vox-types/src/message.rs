//! Spec-level wire types.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and `docs/content/shm-spec/_index.md`.

use std::marker::PhantomData;

use crate::{
    BindingDirection, CborPayload, ChannelId, ConnectionId, Metadata, MethodId, RequestId,
};
use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst, Shape};
use vox_schema::opaque_encoded_borrowed;

/// Per-connection limits advertised by a peer.
// r[impl session.connection-settings]
// r[impl session.parity]
// r[impl connection.parity]
// r[impl rpc.flow-control]
// r[impl rpc.flow-control.max-concurrent-requests]
// r[impl rpc.flow-control.max-concurrent-requests.default]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ConnectionSettings {
    /// Whether this peer will use odd or even IDs for requests and channels on this connection.
    pub parity: Parity,
    /// Maximum number of in-flight requests this peer is willing to accept on this connection.
    pub max_concurrent_requests: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionResumeKey(pub [u8; 16]);

impl<'payload> Message<'payload> {
    // Message has no methods on purpose. it's all just plain data.
    // Adding constructors or getters is forbidden.
}

/// Whether a peer will use odd or even IDs for requests and channels
/// on a given connection.
// r[impl session.parity]
// r[impl session.role]
// r[impl connection.parity]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Parity {
    Odd,
    Even,
}

impl Parity {
    /// Returns the opposite parity.
    pub fn other(self) -> Self {
        match self {
            Parity::Odd => Parity::Even,
            Parity::Even => Parity::Odd,
        }
    }
}

structstruck::strike! {
    /// Protocol message.
    // r[impl session]
    // r[impl session.message]
    // r[impl session.message.connection-id]
    // r[impl session.peer]
    // r[impl session.symmetry]
    #[structstruck::each[derive(Debug, Facet)]]
    pub struct Message<'payload> {
        /// Connection ID: 0 for control messages (ProtocolError, Ping, Pong)
        pub connection_id: ConnectionId,

        /// Message payload
        pub payload:
            #[repr(u8)]
            // r[impl session.message.payloads]
            pub enum MessagePayload<'payload> {
                // ========================================================================
                // Control (conn 0 only)
                // ========================================================================

                /// Sent by either peer when the counterpart has violated the protocol.
                /// The sender closes the transport immediately after sending this message.
                /// No reply is expected or valid.
                // r[impl session.protocol-error]
                ProtocolError(pub struct ProtocolError<'payload> {
                    /// Human-readable description of the protocol violation.
                    pub description: &'payload str,
                }),

                // ========================================================================
                // Connection control
                // ========================================================================

                /// Request a new virtual connection. This is sent on the desired connection
                /// ID, even though it doesn't exist yet.
                // r[impl connection.open]
                // r[impl connection.virtual]
                // r[impl session.connection-settings.open]
                ConnectionOpen(pub struct ConnectionOpen<'payload> {
                    /// Connection limits advertised by the opener.
                    /// Parity is included in ConnectionSettings.
                    pub connection_settings: ConnectionSettings,

                    /// Metadata associated with the connection.
                    pub metadata: Metadata<'payload>,
                }),

                /// Accept a virtual connection request — sent on the connection ID requested.
                // r[impl session.connection-settings.open]
                ConnectionAccept(pub struct ConnectionAccept<'payload> {
                    /// Connection limits advertised by the accepter.
                    pub connection_settings: ConnectionSettings,

                    /// Metadata associated with the connection.
                    pub metadata: Metadata<'payload>,
                }),

                /// Reject a virtual connection request — sent on the connection ID requested.
                ConnectionReject(pub struct ConnectionReject<'payload> {
                    /// Metadata associated with the rejection.
                    pub metadata: Metadata<'payload>,
                }),

                /// Close a virtual connection. Trying to close conn 0 is a protocol error.
                ConnectionClose(pub struct ConnectionClose<'payload> {
                    /// Metadata associated with the close.
                    pub metadata: Metadata<'payload>,
                }),


                // ========================================================================
                // RPC
                // ========================================================================

                RequestMessage(
                    pub struct RequestMessage<'payload> {
                        /// Unique (connection-wide) request identifier, caller-allocated (as per parity)
                        pub id: RequestId,

                        /// Request paylaod
                        pub body:
                            #[repr(u8)]
                            pub enum RequestBody<'payload> {
                                /// Perform a request (or a "call")
                                Call(pub struct RequestCall<'payload> {
                                    /// Unique method identifier, hash of fully qualified name + args etc.
                                    pub method_id: MethodId,

                                    /// Metadata associated with this call
                                    pub metadata: Metadata<'payload>,

                                    /// Argument tuple
                                    pub args: Payload<'payload>,

                                    /// CBOR-encoded schemas for this call's args tuple
                                    /// Non-empty on the first call for each method on a connection.
                                    pub schemas: CborPayload,
                                }),

                                /// Respond to a request
                                Response(struct RequestResponse<'payload> {
                                    /// Arbitrary response metadata
                                    pub metadata: Metadata<'payload>,

                                    /// Return value (`Result<T, VoxError<E>>`, where E could be Infallible depending on signature)
                                    pub ret: Payload<'payload>,

                                    /// CBOR-encoded schemas for this response's return type.
                                    /// Non-empty on the first response for each method on a connection.
                                    pub schemas: CborPayload,
                                }),

                                /// Cancel processing of a request.
                                Cancel(struct RequestCancel<'payload> {
                                    /// Arbitrary cancel metadata
                                    pub metadata: Metadata<'payload>,
                                }),
                            },
                    }
                ),

                /// Advertise schemas for a method binding on this connection.
                ///
                /// This is sent ahead of payload-bearing messages so a batch can
                /// establish all required schema bindings before their first use.
                SchemaMessage(pub struct SchemaMessage {
                    /// Unique method identifier the binding applies to.
                    pub method_id: MethodId,

                    /// Whether the binding applies to request args or responses.
                    pub direction: BindingDirection,

                    /// CBOR-encoded schema payload for this binding.
                    pub schemas: CborPayload,
                }),

                // ========================================================================
                // Channels
                // ========================================================================

                ChannelMessage(
                    pub struct ChannelMessage<'payload> {
                        /// Channel ID (unique per-connection)
                        pub id: ChannelId,

                        /// Channel message body
                        pub body:
                            #[repr(u8)]
                            pub enum ChannelBody<'payload> {
                                /// Send an item on a channel. Channels are not "opened", they are created
                                /// implicitly by calls.
                                Item(pub struct ChannelItem<'payload> {
                                    /// The item itself
                                    pub item: Payload<'payload>,
                                }),

                                /// Close a channel — sent by the sender of the channel when they're gracefully done
                                /// with a channel.
                                Close(pub struct ChannelClose<'payload> {
                                    /// Metadata associated with closing the channel.
                                    pub metadata: Metadata<'payload>,
                                }),

                                /// Reset a channel — sent by the receiver of a channel when they would like the sender
                                /// to please, stop sending items through.
                                Reset(pub struct ChannelReset<'payload> {
                                    /// Metadata associated with resetting the channel.
                                    pub metadata: Metadata<'payload>,
                                }),

                                /// Grant additional send credit to a channel sender.
                                // r[impl rpc.flow-control.credit.grant]
                                GrantCredit(pub struct ChannelGrantCredit {
                                    /// Number of additional items the sender may send.
                                    pub additional: u32,
                                }),
                            },
                    }
                ),

                // ========================================================================
                // Keepalive
                // ========================================================================

                /// Liveness probe for dead-peer detection.
                Ping(pub struct Ping {
                    /// Opaque nonce echoed by the Pong response.
                    pub nonce: u64,
                }),

                /// Reply to a keepalive Ping.
                Pong(pub struct Pong {
                    /// Echo of the received ping nonce.
                    pub nonce: u64,
                }),

            },
    }

}

/// A payload — arguments for a request, or return type for a response.
///
/// Uses `#[facet(opaque = PayloadAdapter)]` so that format crates handle
/// serialization/deserialization through the adapter contract:
/// - **Send path:** `serialize_map` extracts `(ptr, shape)` from `Borrowed` or `Owned`.
/// - **Recv path:** `deserialize_build` produces `RawBorrowed` or `RawOwned`.
// r[impl zerocopy.payload]
#[derive(Debug, Facet)]
#[repr(u8)]
#[facet(opaque = PayloadAdapter, traits(Debug))]
pub enum Payload<'payload> {
    // r[impl zerocopy.payload.borrowed]
    /// Type-erased pointer to caller-owned memory + its Shape.
    Value {
        ptr: PtrConst,
        shape: &'static Shape,
        _lt: PhantomData<&'payload ()>,
    },

    // r[impl zerocopy.payload.bytes]
    /// Raw bytes borrowed from the backing (zero-copy).
    PostcardBytes(&'payload [u8]),
}

impl<'payload> Payload<'payload> {
    /// Construct an outgoing borrowed payload from a concrete value.
    pub fn outgoing<T: Facet<'payload>>(value: &'payload T) -> Self {
        unsafe {
            Self::outgoing_unchecked(PtrConst::new((value as *const T).cast::<u8>()), T::SHAPE)
        }
    }

    /// Construct an outgoing owned payload from a raw pointer + shape.
    ///
    /// # Safety
    ///
    /// The pointed value must remain alive until serialization has completed.
    pub unsafe fn outgoing_unchecked(ptr: PtrConst, shape: &'static Shape) -> Self {
        Self::Value {
            ptr,
            shape,
            _lt: PhantomData,
        }
    }

    /// Create a new `Payload` that borrows the same data with a shorter lifetime.
    ///
    /// For `Outgoing`: same ptr/shape, new lifetime.
    /// For `Incoming`: reborrows the byte slice.
    pub fn reborrow(&self) -> Payload<'_> {
        match self {
            Payload::Value { ptr, shape, .. } => Payload::Value {
                ptr: *ptr,
                shape,
                _lt: PhantomData,
            },
            Payload::PostcardBytes(bytes) => Payload::PostcardBytes(bytes),
        }
    }
}

// SAFETY: The pointer in `Outgoing` is valid for `'payload` and the caller
// guarantees the pointee outlives any use across threads.
unsafe impl<'payload> Send for Payload<'payload> {}

/// Adapter that bridges [`Payload`] through the opaque field contract.
// r[impl zerocopy.framing.value.opaque]
pub struct PayloadAdapter;

impl FacetOpaqueAdapter for PayloadAdapter {
    type Error = String;
    type SendValue<'a> = Payload<'a>;
    type RecvValue<'de> = Payload<'de>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        match value {
            Payload::Value { ptr, shape, .. } => OpaqueSerialize { ptr: *ptr, shape },
            Payload::PostcardBytes(bytes) => opaque_encoded_borrowed(bytes),
        }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        match input {
            OpaqueDeserialize::Borrowed(bytes) => Ok(Payload::PostcardBytes(bytes)),
            OpaqueDeserialize::Owned(_) => {
                Err("payload bytes must be borrowed from backing, not owned".into())
            }
        }
    }
}

/// Type-level tag for [`Message`] as a [`MsgFamily`](crate::MsgFamily).
pub struct MessageFamily;

impl crate::MsgFamily for MessageFamily {
    type Msg<'a> = Message<'a>;
}

// SAFETY: all types below are covariant in their lifetime parameter
// (they contain only Cow<'a, str>, Vec<MetadataEntry<'a>>, etc.).
crate::impl_reborrow!(
    Message,
    RequestMessage,
    RequestCall,
    RequestResponse,
    ConnectionOpen,
    ConnectionAccept,
    ConnectionReject,
    ConnectionClose,
    ChannelMessage,
    ChannelItem,
    ChannelClose,
    ChannelReset,
);
