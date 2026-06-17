//! Spec-level wire types.
//!
//! Canonical definitions live in `docs/content/spec/_index.md`.

use std::marker::PhantomData;

use crate::{BindingDirection, ChannelId, LaneId, Metadata, MethodId, RequestId, SchemaBytes};
use facet::{Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst, Shape};
use vox_phon::raw_opaque_bytes;

/// Default per-channel initial credit and inbound queue capacity.
// r[impl rpc.flow-control.credit.initial]
pub const DEFAULT_INITIAL_CHANNEL_CREDIT: u32 = 16;

/// Per-lane limits advertised by a peer.
// r[impl lane.settings]
// r[impl connection.lane-id-parity]
// r[impl lane.request-channel-parity]
// r[impl rpc.flow-control]
// r[impl rpc.flow-control.max-concurrent-requests]
// r[impl rpc.flow-control.max-concurrent-requests.default]
// r[impl rpc.flow-control.credit.initial]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ConnectionSettings {
    /// Whether this peer will use odd or even IDs for requests and channels on this lane.
    pub parity: Parity,
    /// Maximum number of in-flight requests this peer is willing to accept on this lane.
    pub max_concurrent_requests: u32,
    /// Initial per-channel credit this peer grants for channels it receives.
    #[facet(default = DEFAULT_INITIAL_CHANNEL_CREDIT)]
    pub initial_channel_credit: u32,
}

impl<'payload> Message<'payload> {
    // Message has no methods on purpose. it's all just plain data.
    // Adding constructors or getters is forbidden.
}

/// Whether a peer will use odd or even IDs for requests and channels on a lane.
// r[impl connection.lane-id-parity]
// r[impl connection.role]
// r[impl lane.request-channel-parity]
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
    // r[impl connection.protocol]
    // r[impl connection.message]
    // r[impl connection.message.lane-id]
    // r[impl connection.peer]
    // r[impl connection.symmetry]
    #[structstruck::each[derive(Debug, Facet)]]
    pub struct Message<'payload> {
        /// Lane ID. ID 0 is reserved for connection-control messages.
        pub lane_id: LaneId,

        /// Message payload
        pub payload:
            #[repr(u8)]
            // r[impl connection.message.payloads]
            pub enum MessagePayload<'payload> {
                // ========================================================================
                // Control (conn 0 only)
                // ========================================================================

                /// Sent by either peer when the counterpart has violated the protocol.
                /// The sender closes the transport immediately after sending this message.
                /// No reply is expected or valid.
                // r[impl connection.protocol-error]
                ProtocolError(pub struct ProtocolError<'payload> {
                    /// Human-readable description of the protocol violation.
                    pub description: &'payload str,
                }),

                // ========================================================================
                // Lane control
                // ========================================================================

                // r[impl rpc.metadata.records]
                /// Request a new service lane. This is sent on the desired lane ID,
                /// even though it does not exist yet.
                // r[impl lane.open.wire]
                // r[impl lane.service.compat]
                // r[impl lane.open.settings]
                LaneOpen(pub struct LaneOpen {
                    /// Lane limits advertised by the opener.
                    /// Parity is included in ConnectionSettings.
                    pub connection_settings: ConnectionSettings,

                    /// Metadata associated with the lane.
                    pub metadata: Metadata,
                }),

                /// Accept a lane request, sent on the requested lane ID.
                // r[impl rpc.metadata.records]
                // r[impl lane.open.settings]
                LaneAccept(pub struct LaneAccept {
                    /// Lane limits advertised by the accepter.
                    pub connection_settings: ConnectionSettings,

                    /// Metadata associated with the lane.
                    pub metadata: Metadata,
                }),

                /// Reject a lane request, sent on the requested lane ID.
                // r[impl rpc.metadata.records]
                LaneReject(pub struct LaneReject {
                    /// Metadata associated with the rejection.
                    pub metadata: Metadata,
                }),

                /// Close a service lane. Trying to close lane 0 is a protocol error.
                // r[impl rpc.metadata.records]
                LaneClose(pub struct LaneClose {
                    /// Metadata associated with the close.
                    pub metadata: Metadata,
                }),


                // ========================================================================
                // RPC
                // ========================================================================

                // r[impl rpc.metadata.records]
                RequestMessage(
                    pub struct RequestMessage<'payload> {
                        /// Unique lane-scoped request identifier, caller-allocated (as per parity)
                        pub id: RequestId,

                        /// Request paylaod
                        pub body:
                            #[repr(u8)]
                            pub enum RequestBody<'payload> {
                                /// Perform a request (or a "call")
                                Call(pub struct RequestCall<'payload> {
                                    /// Unique method identifier, hash of service and method names.
                                    // r[impl rpc.method-id]
                                    pub method_id: MethodId,

                                    /// Channel IDs for the `Tx`/`Rx` handles that appear in `args`,
                                    /// allocated by the caller, in encode walk-order. Travels
                                    /// out-of-band from the args payload: each handle encodes only a
                                    /// small index into this list, and the runtime re-associates them
                                    /// at decode (mirrors the `Fd` → fd-table indirection).
                                    // r[impl rpc.request] r[impl rpc.channel.allocation]
                                    pub channels: Vec<ChannelId>,

                                    /// Metadata associated with this call
                                    pub metadata: Metadata,

                                    /// Argument tuple
                                    pub args: Payload<'payload>,

                                    /// phon schema-closure bytes for this call's args tuple.
                                    /// Non-empty on the first call for each method on a connection.
                                    pub schemas: SchemaBytes,
                                }),

                                /// Respond to a request
                                Response(struct RequestResponse<'payload> {
                                    /// Arbitrary response metadata
                                    pub metadata: Metadata,

                                    /// Return value (`Result<T, VoxError<E>>`, where E could be Infallible depending on signature)
                                    pub ret: Payload<'payload>,

                                    /// phon schema-closure bytes for this response's return type.
                                    /// Non-empty on the first response for each method on a connection.
                                    pub schemas: SchemaBytes,
                                }),

                                /// Cancel processing of a request.
                                Cancel(struct RequestCancel {
                                    /// Arbitrary cancel metadata
                                    pub metadata: Metadata,
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

                    /// phon schema-closure bytes for this binding.
                    pub schemas: SchemaBytes,
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
                                Close(pub struct ChannelClose {
                                    /// Metadata associated with closing the channel.
                                    pub metadata: Metadata,
                                }),

                                /// Reset a channel — sent by the receiver of a channel when they would like the sender
                                /// to please, stop sending items through.
                                Reset(pub struct ChannelReset {
                                    /// Metadata associated with resetting the channel.
                                    pub metadata: Metadata,
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
                // r[impl connection.keepalive]
                Ping(pub struct Ping {
                    /// Opaque nonce echoed by the Pong response.
                    pub nonce: u64,
                }),

                /// Reply to a keepalive Ping.
                // r[impl connection.keepalive]
                Pong(pub struct Pong {
                    /// Echo of the received ping nonce.
                    pub nonce: u64,
                }),

            },
    }

}

/// A payload — arguments for a request, or return type for a response.
///
/// Uses `#[facet(opaque = PayloadAdapter)]` so the codec handles
/// serialization/deserialization through the adapter contract:
/// - **Send path:** `serialize_map` either encodes a [`Value`](Payload::Value)'s
///   `(ptr, shape)` or passes an [`Encoded`](Payload::Encoded) span through verbatim.
/// - **Recv path:** `deserialize_build` produces an [`Encoded`](Payload::Encoded)
///   span borrowed from the wire.
#[derive(Debug, Facet)]
#[repr(u8)]
#[facet(opaque = PayloadAdapter, traits(Debug))]
pub enum Payload<'payload> {
    /// Type-erased pointer to caller-owned memory + its Shape, encoded in place.
    Value {
        ptr: PtrConst,
        shape: &'static Shape,
        _lt: PhantomData<&'payload ()>,
    },

    /// Already-encoded payload bytes, borrowed from the backing.
    Encoded(&'payload [u8]),
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
            Payload::Encoded(bytes) => Payload::Encoded(bytes),
        }
    }
}

// SAFETY: The pointer in `Outgoing` is valid for `'payload` and the caller
// guarantees the pointee outlives any use across threads.
unsafe impl<'payload> Send for Payload<'payload> {}

/// Adapter that bridges [`Payload`] through the opaque field contract.
pub struct PayloadAdapter;

impl FacetOpaqueAdapter for PayloadAdapter {
    type Error = String;
    type SendValue<'a> = Payload<'a>;
    type RecvValue<'de> = Payload<'de>;

    fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
        match value {
            Payload::Value { ptr, shape, .. } => OpaqueSerialize { ptr: *ptr, shape },
            Payload::Encoded(bytes) => raw_opaque_bytes(bytes),
        }
    }

    fn deserialize_build<'de>(
        input: OpaqueDeserialize<'de>,
    ) -> Result<Self::RecvValue<'de>, Self::Error> {
        match input {
            OpaqueDeserialize::Borrowed(bytes) => Ok(Payload::Encoded(bytes)),
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
// (they contain only `&'a str`, `Payload<'a>`, etc.).
crate::impl_reborrow!(
    Message,
    RequestMessage,
    RequestCall,
    RequestResponse,
    ChannelMessage,
    ChannelItem,
);

// These payloads carry only owned data now (metadata is a self-describing `Value`,
// settings are `Copy`), so they have no lifetime parameter — their `Reborrow` is the
// identity. They are still projected out of a `SelfRef` (the connection runtime pattern-matches
// them), which requires the impl.
// SAFETY: owned types with no lifetime parameter; `Ref<'a> = Self` is trivially sound.
macro_rules! impl_reborrow_owned {
    ($($ty:ident),* $(,)?) => {
        $(
            unsafe impl crate::Reborrow for $ty {
                type Ref<'a> = $ty;
            }
        )*
    };
}
impl_reborrow_owned!(
    LaneOpen,
    LaneAccept,
    LaneReject,
    LaneClose,
    ChannelClose,
    ChannelReset,
    SchemaMessage,
);

#[cfg(test)]
mod tests {
    use super::{ChannelBody, MessagePayload, RequestBody};
    use facet::Facet;
    use facet_core::{Type, UserType};

    fn enum_variant_names<T: Facet<'static>>() -> Vec<&'static str> {
        match T::SHAPE.ty {
            Type::User(UserType::Enum(enum_type)) => enum_type
                .variants
                .iter()
                .map(|variant| variant.name)
                .collect(),
            other => panic!("expected enum shape, got {other:?}"),
        }
    }

    // r[verify connection.message.payloads]
    #[test]
    fn message_payload_shape_lists_compact_session_payloads() {
        let payloads = enum_variant_names::<MessagePayload<'static>>();
        assert_eq!(
            payloads,
            [
                "ProtocolError",
                "LaneOpen",
                "LaneAccept",
                "LaneReject",
                "LaneClose",
                "RequestMessage",
                "SchemaMessage",
                "ChannelMessage",
                "Ping",
                "Pong",
            ]
        );

        for handshake_variant in ["Hello", "HelloYourself", "LetsGo", "Decline", "Sorry"] {
            assert!(
                !payloads.contains(&handshake_variant),
                "{handshake_variant} is a phon handshake message, not a compact MessagePayload"
            );
        }

        assert_eq!(
            enum_variant_names::<RequestBody<'static>>(),
            ["Call", "Response", "Cancel"]
        );
        assert_eq!(
            enum_variant_names::<ChannelBody<'static>>(),
            ["Item", "Close", "Reset", "GrantCredit"]
        );
    }
}
