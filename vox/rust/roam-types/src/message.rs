//! Spec-level wire types.
//!
//! Canonical definitions live in `docs/content/spec/_index.md` and `docs/content/shm-spec/_index.md`.

use std::marker::PhantomData;

use crate::{ChannelId, ConnectionId, Metadata, MethodId, RequestId};
use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst, Shape};

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
        /// Connection ID: 0 for control messages (Hello, HelloYourself)
        pub connection_id: ConnectionId,

        /// Message payload
        pub payload:
            #[repr(u8)]
            // r[impl session.message.payloads]
            pub enum MessagePayload<'payload> {
                // ========================================================================
                // Control (conn 0 only)
                // ========================================================================

                /// Sent by initiator to acceptor as the first message
                // r[impl session.handshake]
                // r[impl session.connection-settings.hello]
                Hello(pub struct Hello<'payload> {
                    /// Must be equal to 7
                    pub version: u32,

                    /// Connection limits advertised by the initiator for the root connection.
                    /// Parity is included in ConnectionSettings.
                    pub connection_settings: ConnectionSettings,

                    /// Metadata associated with the connection.
                    pub metadata: Metadata<'payload>,
                }),

                /// Sent by acceptor back to initiator. Poetic on purpose, I'm not changing the name.
                // r[impl session.connection-settings.hello]
                HelloYourself(pub struct HelloYourself<'payload> {
                    /// Connection limits advertised by the acceptor for the root connection.
                    pub connection_settings: ConnectionSettings,

                    /// You can _also_ have metadata if you want.
                    pub metadata: Metadata<'payload>,
                }),

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

                                    /// Channel identifiers, allocated by the caller, that are passed as part
                                    /// of the arguments.
                                    pub channels: Vec<ChannelId>,

                                    /// Metadata associated with this call
                                    pub metadata: Metadata<'payload>,

                                    /// Argument tuple
                                    #[facet(trailing)]
                                    pub args: Payload<'payload>,
                                }),

                                /// Respond to a request
                                Response(struct RequestResponse<'payload> {
                                    /// Channel IDs for streams in the response, in return type declaration order.
                                    pub channels: Vec<ChannelId>,

                                    /// Arbitrary response metadata
                                    pub metadata: Metadata<'payload>,

                                    /// Return value (`Result<T, RoamError<E>>`, where E could be Infallible depending on signature)
                                    #[facet(trailing)]
                                    pub ret: Payload<'payload>,
                                }),

                                /// Cancel processing of a request.
                                Cancel(struct RequestCancel<'payload> {
                                    /// Arbitrary cancel metadata
                                    pub metadata: Metadata<'payload>,
                                }),
                            },
                    }
                ),

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
                                    #[facet(trailing)]
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
                //
                // NOTE: these variants are intentionally appended to preserve
                // existing discriminants for earlier message payload variants.

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
    /// Outgoing: type-erased pointer to caller-owned memory + its Shape.
    Outgoing {
        ptr: PtrConst,
        shape: &'static Shape,
        _lt: PhantomData<&'payload ()>,
    },

    // r[impl zerocopy.payload.bytes]
    /// Incoming: raw bytes borrowed from the backing (zero-copy).
    Incoming(&'payload [u8]),
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
        Self::Outgoing {
            ptr,
            shape,
            _lt: PhantomData,
        }
    }

    // ps: as_incoming_bytes was a bad idea. it's not here anymore.
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
        let Payload::Outgoing { ptr, shape, .. } = value else {
            unreachable!("serialize_map is only called on outgoing Payload variants");
        };
        OpaqueSerialize { ptr: *ptr, shape }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        match input {
            OpaqueDeserialize::Borrowed(bytes) => Ok(Payload::Incoming(bytes)),
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
